

// # ws-dev.conf — one server, two listeners
// listen: 127.0.0.1:4222

// websocket {
    //   listen: 127.0.0.1:8080
    //   no_tls: true
    // }
    
    // Bluesky link?
    //wss://jetstream2.us-east.bsky.network/subscribe"
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
// use rustls::
// CryptoProvider;
use tokio_tungstenite::tungstenite::http::{Method, Request};
use tokio_tungstenite::connect_async;
use futures_util::{StreamExt, stream};

#[tokio::main]
async fn main() {


    // let mut request = "wss://jetstream2.us-east.bsky.network/subscribe".into_client_request().unwrap();
    let mut request = "wss://jetstream.us-east.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections=app.bsky.feed.post".into_client_request().unwrap();
    request.headers_mut().insert("Sec-WebSocket-Protocol", "xrpc.v1.json".parse().unwrap());
    
    let (mut stream, _response) = connect_async(request).await.unwrap();

    while let Some(msg) = stream.next().await{
        let msg = msg.expect("We got an error here");
        println!("{msg}");
    } 
}
