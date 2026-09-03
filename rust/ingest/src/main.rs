use tokio_tungstenite::tungstenite::client::IntoClientRequest;
// use rustls::
// CryptoProvider;
use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
// use tokio_tungstenite::tungstenite::http::{Method, Request};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::protocol::Message;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let _log_level = std::env::var("LOG_LEVEL");
    let js_collection = std::env::var("JETSTREAM_COLLECTIONS").unwrap();
    let zone = std::env::var("ZONE").unwrap();

    // let mut request = "wss://jetstream2.us-east.bsky.network/subscribe".into_client_request().unwrap();
    let mut request = format!("wss://jetstream.{zone}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={js_collection}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Sec-WebSocket-Protocol", "xrpc.v1.json".parse().unwrap());

    let (mut stream, _response) = connect_async(request).await.unwrap();

    while let Some(msg) = stream.next().await {
        let msg = msg.expect("We got an error here");
        println!("--New message");

        // call a function here instead
        match msg {
            Message::Text(text) => {
                // println!("{text}");
                // let parsed: JetstreamMessage = serde_json::from_str(&text).expect(JetstreamMessage);
                let parsed: JetstreamMessage = serde_json::from_str(&text).unwrap();
                println!("\tseq={}", parsed.payload.seq);
                // let text = parsed.payload.record.text;
                // println!("\ttext={}", parsed.payload.record.text);

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
