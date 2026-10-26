use ingest::config::{Config, Rotation};
use ingest::producer::EventProducer;
use murmur_common::jetstream::JetstreamMessage;

use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, error, info, trace, warn};

// File Writing
use chrono::Utc;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zstd::{Decoder, Encoder};

/// Chosen by `ROTATION` in the environment; see `config::Rotation`.
fn rotation_format(rotation: &Rotation) -> &'static str {
    match rotation {
        Rotation::Minutely => "%Y-%m-%dT%H%M",
        Rotation::Hourly => "%Y-%m-%dT%H",
        Rotation::Daily => "%Y-%m-%d",
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ########## Connection section ##########
    let config: Config = murmur_common::telemetry::bootstrap()?;

    // Kafka start up
    let event_producer = EventProducer::new(&config.kafka_brokers, &config.kafka_events_topic)?;

    let backoff = BackoffConfig {
        base_delay_secs: config.base_delay_secs,
        max_delay_secs: config.max_delay_secs,
        multiplier: config.backoff_multiplier.get(),
    };
    let mut consecutive_failures: u32 = 0;
    let url = config.jetstream_url();

    let rotation_format = rotation_format(&config.rotation);

    // ########## File section ##########
    std::fs::create_dir_all(&config.readings_dir)?;
    let mut archive =
        create_file(&config.readings_dir, rotation_format).expect("could not create archive file");

    // ########## Connection Loop ##########
    loop {
        if consecutive_failures > 0 {
            sleep(backoff_delay(&backoff, consecutive_failures)).await
        }

        let connection_result = connect(&url).await;
        let mut stream = match connection_result {
            Ok(s) => {
                consecutive_failures = 0;
                s
            }
            Err(e) => {
                warn!("connect failed: {e}");
                consecutive_failures += 1;
                continue;
            }
        };

        // ########## Message Loop ##########
        loop {
            // Is this a big toll on a process that is meant to be as quick as possible
            if archive.latest_file_time != Utc::now().format(rotation_format).to_string() {
                info!("rotating to a new archive file");

                // new file
                match create_file(&config.readings_dir, rotation_format) {
                    Ok(next_archive) => {
                        let finished = std::mem::replace(&mut archive, next_archive);
                        match finished.writer.into_inner() {
                            Ok(file) => {
                                drop(file);
                                //TODO(compress): hand finished.archive_name to the
                                //background compression task.
                                if let Err(e) = compress_archive(&finished.full_path) {
                                    error!(
                                        "could not compress {}: {}",
                                        finished.full_path.display(),
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                error!(
                                    "could not flush finished archive {}: {}",
                                    finished.full_path.display(),
                                    e.error()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("could not create the next archive file: {e}");
                        // keep printing into the old file instead
                        archive.latest_file_time = Utc::now().format(rotation_format).to_string();
                    }
                }
            }
            match timeout(
                Duration::from_secs(config.missed_pings_tolerance.get() * 30),
                stream.next(),
            )
            .await
            {
                Ok(next_message) => match next_message {
                    Some(Ok(Message::Close(_))) => {
                        warn!("server closed the connection");
                        break;
                    }
                    Some(Ok(msg)) => {
                        handle_message(msg, &mut archive, &event_producer).await;
                    }
                    Some(Err(e)) => {
                        warn!("stream error: {e}");
                        break;
                    }
                    None => {
                        warn!("stream ended");
                        break;
                    }
                },
                Err(timed_out_error) => {
                    error!(
                        "missed {} pings, disconnecting: {}",
                        config.missed_pings_tolerance, timed_out_error
                    );
                    break;
                }
            }
        }
        // Falling out of the inner loop always means the connection ended.
        // Program never gives up: with no supervisor to restart and no alerting,
        // stopping would just leave the process silently dead. Capped delay,
        // uncapped attempts.
        consecutive_failures += 1
    }
}

async fn connect(url: &str) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
    let mut request = url.into_client_request()?;
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "xrpc.v1.json".parse().unwrap());
    let (stream, _response) = connect_async(request).await?;

    Ok(stream)
}

fn create_file(dir: &Path, rotation_format: &str) -> Result<Archive, std::io::Error> {
    let latest_file_time = Utc::now().format(rotation_format).to_string();
    let archive_name = format!("logs_{}.jsonl", latest_file_time); // 2026-09-09T16.jsonl

    let path: PathBuf = dir.join(&archive_name);

    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let archive = Archive {
        writer: BufWriter::new(file),
        latest_file_time,
        full_path: path,
    };
    Ok(archive)
}

fn compress_archive(path: &Path) -> anyhow::Result<()> {
    let tmp_path = path.with_extension("jsonl.zst.tmp");
    let final_path = path.with_extension("jsonl.zst");

    let mut source = File::open(path)?;
    let destination = File::create(&tmp_path)?;

    let mut encoder = Encoder::new(destination, 0)?;
    encoder.include_checksum(true)?;

    // The line that actually compresses. The encoder is itself a writer, so
    // every byte copied into it comes out compressed into the temp file.
    let number_bytes_compressed = io::copy(&mut source, &mut encoder)?;

    // Required: writes the zstd frame epilogue and hands the File back.
    let destination = encoder.finish()?;
    destination.sync_all()?;
    drop(destination);

    let bytes_out = std::fs::metadata(&tmp_path)?.len();
    info!(
        "compressed {} -> {} ({number_bytes_compressed} -> {bytes_out} bytes)",
        path.display(),
        tmp_path.display()
    );

    if let Err(e) = verify_compression(number_bytes_compressed, &tmp_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }

    std::fs::rename(&tmp_path, &final_path)?;

    std::fs::remove_file(path)?;
    Ok(())
}

// Uncompress
fn verify_compression(initial_byte_count: u64, compressed_file_path: &Path) -> anyhow::Result<()> {
    let source = File::open(compressed_file_path)?;
    let mut decoder = Decoder::new(source)?;

    let bytes_uncompressed = io::copy(&mut decoder, &mut io::sink())?;

    anyhow::ensure!(
        bytes_uncompressed == initial_byte_count,
        "verification failed for {}: expected {initial_byte_count}, got {bytes_uncompressed}",
        compressed_file_path.display()
    );
    debug!("Verification completed. # bytes Before {initial_byte_count}, got {bytes_uncompressed}");

    Ok(())
}

fn backoff_delay(config: &BackoffConfig, attempts: u32) -> Duration {
    let growth = config.multiplier.saturating_pow(attempts);
    let computed = config.base_delay_secs.saturating_mul(growth);
    let delay = computed.min(config.max_delay_secs);

    // TODO(jitter): randomize within the delay so independent clients
    // don't all retry in lockstep against a shared public service.
    Duration::from_secs(delay)
}

async fn handle_message(msg: Message, archive: &mut Archive, event_producer: &EventProducer) -> () {
    match msg {
        Message::Text(raw) => {
            trace!("{raw}");

            // write to disk
            if let Err(e) = archive.writer.write_all(raw.as_bytes()) {
                error!("Could not write to disk: {e}");
            }
            if let Err(e) = archive.writer.write_all(b"\n") {
                error!("Could not write to disk: {e}");
            }

            let parsed: JetstreamMessage = match serde_json::from_str(&raw) {
                Ok(message) => message,
                Err(e) => {
                    error!("could not parse event, archived but not produced: {e}");
                    return;
                }
            };

            debug!("\tseq={}", parsed.payload.seq);

            if let Some(record) = &parsed.payload.record {
                if let Some(text) = &record.text {
                    debug!("\ttext={text}");
                }
            }
            // Send to Kafka
            if let Err(e) = event_producer.send(&parsed.payload.uri(), &raw).await {
                error!("could not produce {}: {e:#}", parsed.payload.uri());
            }
        }
        Message::Ping(_payload) => {
            debug!("ping at {}", Utc::now().format("%Y-%m-%dT%H%M"))
        }
        Message::Pong(_payload) => {}
        Message::Close(_frame) => {}
        Message::Frame(_) => {}
        Message::Binary(_) => {}
    }
}

struct BackoffConfig {
    base_delay_secs: u64,
    max_delay_secs: u64,
    multiplier: u64,
}

struct Archive {
    writer: BufWriter<File>,
    latest_file_time: String,
    full_path: PathBuf,
}
