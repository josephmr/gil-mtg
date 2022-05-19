use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use thiserror::Error;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
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

#[derive(Error, Debug)]
enum Error {
    #[error("unknown websocket error")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("invalid message json")]
    Json {
        #[from]
        source: serde_json::Error,
    },
    #[error("invalid Guilded bearer token")]
    InvalidToken,
}

struct Client {
    token: String,
}

type LibResult<T> = core::result::Result<T, Error>;

type Events = UnboundedReceiver<Event>;

impl Client {
    async fn new(token: &str) -> LibResult<Client> {
        Ok(Client {
            token: token.to_string(),
        })
    }

    // TODO: Use type state to manage connection state (connected|connecting|not_connected)
    async fn start(&mut self) -> LibResult<Events> {
        let mut req = "wss://api.guilded.gg/v1/websocket"
            .into_client_request()
            .unwrap();
        req.headers_mut().append(
            "Authorization",
            format!("Bearer {}", self.token,)
                .parse()
                .map_err(|_| Error::InvalidToken)?,
        );
        let (tx, rx) = unbounded_channel();

        let (mut ws_stream, _) = connect_async(req).await?;
        tokio::spawn(async move {
            while let Some(message) = ws_stream.next().await {
                if let Some(event) = Client::parse(message.unwrap()) {
                    tx.send(event).unwrap();
                }
            }
        });
        Ok(rx)
    }

    fn parse(message: tokio_tungstenite::tungstenite::Message) -> Option<Event> {
        match message {
            WsMessage::Text(message) => {
                if let Op { op: OpCode::Event } = serde_json::from_str(&message).unwrap() {
                    Some(serde_json::from_str::<Message>(&message).unwrap().d)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    let mut client = Client::new(&std::env::var("GUILDED_BEARER_TOKEN").unwrap()).await?;
    let mut events = client.start().await?;
    loop {
        let event = events.recv().await;
        println!("{:?}", event);
    }
}
