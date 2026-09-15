mod config;

use rustls::server::Accepted;
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
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let debug_format = "%Y-%m-%dT%H%M";
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
            if archive.latest_file_time != Utc::now().format(debug_format).to_string() {
                println!("🚨\n🚨\n🚨\n🚨NEW FILE🚨\n🚨\n🚨\n🚨\n");
                // archive
                //     .writer
                //     .flush()
                //     .expect("Couldn't finish writing to file");

                // new file
                match create_file(&config.readings_dir) {
                    Ok(next_archive) => {
                        let finished = std::mem::replace(&mut archive, next_archive);
                        match finished.writer.into_inner() {
                            Ok(file) => {
                                drop(file);
                                //TODO(compress): hand finished.archive_name to the
                                //background compression task.
                            }
                            Err(e) => {
                                eprintln!(
                                    "could not flush finished archive {}: {}",
                                    finished.archive_name,
                                    e.error()
                                );
                            }
                        }
                    }
                    Err(e) => {
                        println!("error:{}", e);
                        // keep printing into the old file instead
                        archive.latest_file_time = Utc::now().format(debug_format).to_string();
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
    let debug_format = "%Y-%m-%dT%H%M";

    let latest_file_time = Utc::now().format(debug_format).to_string();
    let archive_name = format!("logs_{}.jsonl", latest_file_time); // 2026-09-09T16.jsonl

    let path = dir.join(&archive_name);

    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let archive = Archive {
        writer: BufWriter::new(file),
        archive_name: archive_name,
        latest_file_time: latest_file_time,
    };
    return Ok(archive);
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
    archive_name: String,
    latest_file_time: String,
}
