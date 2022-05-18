use dotenv;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Message as WsMessage,
};

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug)]
#[repr(u8)]
enum OpCode {
    Event = 0,
    Welcome = 1,
    Resume = 2,
}

#[derive(Serialize, Deserialize, Debug)]
struct Op {
    op: OpCode,
}

type MessageId = String;

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    #[serde(flatten)]
    op: Op,
    #[serde(flatten)]
    d: Event,
    s: MessageId,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "t", content = "d")]
enum Event {
    #[serde(rename_all = "camelCase")]
    ChatMessageCreated {
        server_id: String,
        message: ChatMessage,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    content: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let mut req = "wss://api.guilded.gg/v1/websocket"
        .into_client_request()
        .unwrap();
    req.headers_mut().append(
        "Authorization",
        format!(
            "Bearer {}",
            std::env::var("GUILDED_BEARER_TOKEN").expect("GUILDED_BEARER_TOKEN must be specified")
        )
        .parse()
        .expect("GUILD_BEARER_TOKEN must be a valid token"),
    );

    let (ws_stream, _) = connect_async(req).await?;
    let (_, mut read) = ws_stream.split();

    while let Some(message) = read.next().await {
        let message = message?;
        println!("handling: {:?}", message);
        match message {
            WsMessage::Text(message) => {
                if let Op { op: OpCode::Event } = serde_json::from_str(&message)? {
                    println!(
                        "GOT: {:?}",
                        serde_json::from_str::<Message>(&message).unwrap()
                    );
                }
            }
            _ => (),
        }
    }

    Ok(())
}
