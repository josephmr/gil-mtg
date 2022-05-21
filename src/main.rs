use futures_util::StreamExt;
use mtg::http::{ChatEmbed, UrlWrapper};
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::sync::Mutex;
use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Message as WsMessage,
};
use tracing::{debug, error};

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
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    channel_id: String,
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
    sender: Mutex<Option<UnboundedSender<Event>>>,
    token: String,
}

type LibResult<T> = core::result::Result<T, Error>;

type Events = UnboundedReceiver<Event>;

impl Client {
    async fn new(token: &str) -> LibResult<Client> {
        Ok(Client {
            sender: Mutex::new(None),
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
        self.sender = Mutex::new(Some(tx));
        let sender = self.sender.lock().expect("sender poisoned").take().unwrap(); // TODO: handle already started -- take() will return None

        let (mut ws_stream, _) = connect_async(req).await?;
        tokio::spawn(async move {
            while let Some(message) = ws_stream.next().await {
                if let Some(event) = Client::parse(message.unwrap()) {
                    sender.send(event).unwrap();
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
    tracing_subscriber::fmt::init();

    let token = std::env::var("GUILDED_BEARER_TOKEN").unwrap();
    let mut client = Client::new(&token).await?;
    let mut events = client.start().await?;
    debug!("listening");
    use mtg::http::Client as HttpClient;
    let http_client = HttpClient::new(&token);
    loop {
        if let Some(Event::ChatMessageCreated { message, .. }) = events.recv().await {
            let http_client = http_client.clone();
            tokio::spawn(async move {
                match mtg::scryfall::find(&message.content).await {
                    Ok(Some(card)) => {
                        let image_url = card.image_uris.map(|c| c.small);
                        debug!("Found card: {} {:?}", card.name, image_url);
                        let res = http_client
                            .create_message(
                                message.channel_id,
                                mtg::http::Message {
                                    embeds: Some(vec![ChatEmbed {
                                        title: Some(card.name),
                                        image: Some(UrlWrapper { url: image_url }),
                                        ..Default::default()
                                    }]),
                                    ..Default::default()
                                },
                            )
                            .await;
                        match res {
                            Ok(_) => debug!("create_message success"),
                            Err(err) => error!(?err, "create_message failed"),
                        }
                    }
                    Ok(None) => {
                        debug!("found no matching card for {name}", name = &message.content)
                    }
                    Err(err) => debug!(?err),
                }
            });
        }
    }
}
