use dotenv;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_repr::*;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Message as WsMessage,
    MaybeTlsStream, WebSocketStream,
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
    pub ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    token: String,
}

impl Stream for Client {
    type Item = Event;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.ws_stream.poll_next_unpin(cx) {
            std::task::Poll::Ready(message) => {
                let message = message.unwrap().unwrap();
                let event = match message {
                    WsMessage::Text(message) => {
                        if let Op { op: OpCode::Event } = serde_json::from_str(&message).unwrap() {
                            Some(serde_json::from_str::<Message>(&message).unwrap().d)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                std::task::Poll::Ready(event)
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

type LibResult<T> = core::result::Result<T, Error>;

impl Client {
    async fn new(token: &str) -> LibResult<Client> {
        let mut req = "wss://api.guilded.gg/v1/websocket"
            .into_client_request()
            .unwrap();
        req.headers_mut().append(
            "Authorization",
            format!("Bearer {}", token,)
                .parse()
                .map_err(|_| Error::InvalidToken)?,
        );
        let (ws_stream, _) = connect_async(req).await?;
        Ok(Client {
            ws_stream,
            token: token.to_string(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    let mut client = Client::new(&std::env::var("GUILDED_BEARER_TOKEN").unwrap()).await?;
    loop {
        let event = client.next().await;
        println!("{:?}", event);
    }
}
