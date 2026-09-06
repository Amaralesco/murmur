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

    let url = format!("wss://jetstream.{zone}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={js_collection}");
    loop {
        let connection_result = connect(&url).await;
        let mut stream = match connection_result {
            Ok(s) => s,
            Err(e) => {
                eprintln!("connect failed: {e}");
                sleep(Duration::from_secs(5)).await;
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
                    eprintln!("stream error: {e}");
                    break;
                }
                None => {
                    eprintln!("stream ended");
                    break;
                }
            }
        }
        // TODO(backoff): grow the delay across consecutive failures, reset on success, cap it
        sleep(Duration::from_secs(5)).await;
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
    //   time: String;//help me //Can i name it timestamp instead?
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
