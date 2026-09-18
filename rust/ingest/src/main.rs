mod config;

use futures_util::future::ok;
use rustls::server::Accepted;
use rustls::Writer;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing_subscriber::fmt::time;

use crate::config::Config;
use envconfig::Envconfig;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::protocol::Message;

use core::arch;
// File Writing
use chrono::{DateTime, Utc};
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use zstd::{Decoder, Encoder};

/// Archive rotation boundary. Zero-padded and most-significant-first, so
/// lexicographic order matches chronological order. UTC, so daylight saving
/// cannot produce two files with the same name.
///
/// Both the file name and the boundary check derive from this one string: if
/// they ever disagreed, rotation would fire against a name it did not create.
/// Append `%M` to rotate every minute when testing.
const ROTATION_FORMAT: &str = "%Y-%m-%dT%H";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TEMPORARY slice-2 test. Delete once compression runs from rotation.
    // Note the argument is the SOURCE .jsonl; the .tmp name is derived inside.
    // compress_archive(Path::new("data/tmp/logs_2026-09-14T1155.jsonl"))?;
    // compress_archive(Path::new("data/tmp/logs_2026-09-14T1155.jsonl"))?;
    // return Ok(());

    // ########## Connection section ##########
    dotenvy::dotenv().ok();

    let config = Config::init_from_env().expect("invalid configuration");

    let backoff = BackoffConfig {
        base_delay_secs: config.base_delay_secs,
        max_delay_secs: config.max_delay_secs,
        multiplier: config.backoff_multiplier.get(),
    };
    let mut consecutive_failures: u32 = 0;
    let url = config.jetstream_url();

    // ########## File section ##########
    std::fs::create_dir_all(&config.readings_dir)?;
    let mut archive = create_file(&config.readings_dir).expect("could not create archive file");

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
                eprintln!("connect failed: {e}");
                consecutive_failures += 1;
                continue;
            }
        };

        // ########## Message Loop ##########
        loop {
            // Is this a big toll on a process that is meant to be as quick as possible
            if archive.latest_file_time != Utc::now().format(ROTATION_FORMAT).to_string() {
                println!("🚨\n🚨\n🚨\n🚨NEW FILE🚨\n🚨\n🚨\n🚨\n");

                // new file
                match create_file(&config.readings_dir) {
                    Ok(next_archive) => {
                        let finished = std::mem::replace(&mut archive, next_archive);
                        match finished.writer.into_inner() {
                            Ok(file) => {
                                drop(file);
                                //TODO(compress): hand finished.archive_name to the
                                //background compression task.
                                if let Err(e) = compress_archive(&finished.full_path) {
                                    eprintln!(
                                        "could not compress {}: {}",
                                        finished.full_path.display(),
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "could not flush finished archive {}: {}",
                                    finished.full_path.display(),
                                    e.error()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("error:{}", e);
                        // keep printing into the old file instead
                        archive.latest_file_time = Utc::now().format(ROTATION_FORMAT).to_string();
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
                        eprintln!("server closed the connection");
                        break;
                    }
                    Some(Ok(msg)) => {
                        handle_message(msg, &mut archive);
                    }
                    Some(Err(e)) => {
                        eprintln!("stream error: {e}");
                        break;
                    }
                    None => {
                        eprintln!("stream ended");
                        break;
                    }
                },
                Err(timed_out_error) => {
                    println!(
                        "WARN: Connection pinged missed {} times",
                        config.missed_pings_tolerance
                    );
                    println!("WARN: Disconnecting, error {}", timed_out_error);
                    break;
                }
            }
        }
        // Falling out of the inner loop always means the connection ended.
        // Program never gives up: with no supervisor to restart and no alerting,
        // stopping would just leave the process silently dead. Capped delay,
        // uncapped attempts.
        consecutive_failures += 1 // ← this line
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

fn create_file(dir: &Path) -> Result<Archive, std::io::Error> {

    let latest_file_time = Utc::now().format(ROTATION_FORMAT).to_string();
    let archive_name = format!("logs_{}.jsonl", latest_file_time); // 2026-09-09T16.jsonl

    let path: PathBuf = dir.join(&archive_name);

    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let archive = Archive {
        writer: BufWriter::new(file),
        latest_file_time: latest_file_time,
        full_path: path,
    };
    return Ok(archive);
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
    println!(
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
    println!(
        "Verification completed. # bytes Before {initial_byte_count}, got {bytes_uncompressed}"
    );

    Ok(())
}

fn backoff_delay(config: &BackoffConfig, attempts: u32) -> Duration {
    let growth = config.multiplier.saturating_pow(attempts);
    let computed = config.base_delay_secs.saturating_mul(growth);
    let delay = computed.min(config.max_delay_secs);

    // TODO(jitter): randomize within the delay so independent clients
    // don't all retry in lockstep against a shared public service.
    return Duration::from_secs(delay);
}

fn handle_message(msg: Message, archive: &mut Archive) -> () {
    match msg {
        Message::Text(text) => {
            println!("{text}");
            let parsed: JetstreamMessage = serde_json::from_str(&text).unwrap();
            println!("\tseq={}", parsed.payload.seq);

            if let Some(record) = &parsed.payload.record {
                if let Some(text) = &record.text {
                    println!("\ttext={text}");
                }
            }
            if let Err(e) = archive.writer.write_all(text.as_bytes()) {
                eprintln!("Could not write to disk: {e}");
                // return Err(e);
            }
            if let Err(e) = archive.writer.write_all(b"\n") {
                eprintln!("Could not write to disk: {e}");
            }
        }
        Message::Ping(_payload) => {
            println!(
                "{}",
                format!(
                    "🔵PING PING PING🔵 at {}",
                    Utc::now().format("%Y-%m-%dT%H%M")
                )
            )
        }
        Message::Pong(_payload) => {}
        Message::Close(_frame) => {
            // frame is Option<CloseFrame>
        }
        Message::Frame(_) => {}
        Message::Binary(_) => {}
    }
}

#[derive(Deserialize)]
struct JetstreamMessage {
    payload: Commit,
    //   phones: Vec<String>,
    //   opreation: Enum <create,post,commit>
}
#[derive(Deserialize)]
struct Commit {
    record: Option<Record>,
    cid: Option<String>,
    did: String,
    seq: u64,
    operation: Operation,
}
#[derive(Deserialize)]
struct Record {
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Operation {
    Create,
    Update,
    Delete,
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
