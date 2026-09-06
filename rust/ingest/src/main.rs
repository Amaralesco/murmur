use std::time::Duration;

use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

// use rustls::
// CryptoProvider;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio_tungstenite::tungstenite::protocol::Message;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let _log_level = std::env::var("LOG_LEVEL");
    let js_collection = std::env::var("JETSTREAM_COLLECTIONS").unwrap();
    let zone = std::env::var("ZONE").unwrap();

    let mut config = BackoffConfig {
        base_delay_secs: std::env::var("BASE_DELAY_SECS").unwrap().parse().unwrap(),
        max_delay_secs: std::env::var("MAX_DELAY_TIME").unwrap().parse().unwrap(),
        multiplier: std::env::var("BACKOFF_MULTIPLIER")
            .unwrap()
            .parse()
            .unwrap(),
    };
    let mut consecutive_failures: u32 = 0;

    let url = format!("wss://jetstream.{zone}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={js_collection}");
    loop {
        if consecutive_failures > 0 {
            sleep(backoff_delay(&config, consecutive_failures)).await
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

        loop {
            match stream.next().await {
                Some(Ok(Message::Close(_))) => {
                    eprintln!("server closed the connection");
                    break;
                }
                Some(Ok(msg)) => handle_message(msg),
                Some(Err(e)) => {
                    // TODO(backoff): no need to call it, cause its caught outside the loop
                    eprintln!("stream error: {e}");
                    break;
                }
                None => {
                    eprintln!("stream ended");
                    break;
                }
            }
        }
        // Falling out of the inner loop always means the connection ended.
        // We never give up: with no supervisor to restart us and no alerting,
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

fn backoff_delay(config: &BackoffConfig, attempts: u32) -> Duration {
    let growth = config.multiplier.saturating_pow(attempts);
    let computed = config.base_delay_secs.saturating_mul(growth);
    let delay = computed.min(config.max_delay_secs);

    // TODO(jitter): randomize within the delay so independent clients
    // don't all retry in lockstep against a shared public service.
    return Duration::from_secs(delay);
}

fn handle_message(msg: Message) -> () {
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
        }
        Message::Ping(_payload) => {}
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
