use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Error as TsError,
    tungstenite::Message as TsMessage, MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};

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

#[derive(Error, Debug)]
enum ConnectingError {
    #[error("invalid Guilded bearer token")]
    InvalidToken,
    #[error("error establishing websocket connection")]
    Establishing(#[from] tokio_tungstenite::tungstenite::Error),
}

struct Client {
    sender: Mutex<Option<UnboundedSender<Event>>>,
    token: String,
}

enum ProcessorAction {}

struct Processor {
    events: UnboundedSender<Event>,
    controls: UnboundedSender<ProcessorAction>,
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Processor {
    async fn new(
        token: &str,
        events: UnboundedSender<Event>,
        controls: UnboundedSender<ProcessorAction>,
    ) -> Result<Self, ConnectingError> {
        let mut req = "wss://api.guilded.gg/v1/websocket"
            .into_client_request()
            .expect("failed to parse websocker uri");
        req.headers_mut().append(
            "Authorization",
            format!("Bearer {}", token,)
                .parse()
                .map_err(|_| ConnectingError::InvalidToken)?,
        );
        debug!("processor attempting to connect to Guilded websocket");
        let (ws_stream, _) = connect_async(req).await?;
        debug!("processor websocket connected successfully");

        Ok(Processor {
            events,
            controls,
            ws_stream,
        })
    }

    async fn run(&mut self) {
        while let Some(message) = self.ws_stream.next().await {
            match message {
                Ok(message) => self.handle_message(message),
                Err(_) => todo!(),
            }
        }

        // Received a none, this should be interpreted as a force close of the
        // socket and attempt a reconnect
        error!("processor stream ended unexpectedly")
    }

    fn handle_event(&self, text: String) {
        // First pass just decode the "op" code from the event. This determined
        // if it will fully parse into a Message or if it is a Welcome message.
        let op = serde_json::from_str::<Op>(&text);
        match op {
            Ok(Op { op: OpCode::Event }) => match serde_json::from_str::<Message>(&text) {
                Ok(event) => self
                    .events
                    .send(event.d)
                    .expect("processor failed to send event"),
                Err(err) => warn!(?text, ?err, "failed to decode event"),
            },
            Ok(Op {
                op: OpCode::Welcome,
            }) => debug!(?text, "received welcome message"),
            Ok(op) => warn!(?text, ?op, "failed to decode event, unrecognized op code"),
            Err(err) => error!(?err, "failed to decode Op"),
        }
    }

    fn handle_message(&self, message: TsMessage) {
        match message {
            TsMessage::Text(text) => self.handle_event(text),
            // TODO: implement client side heartbeats + pong handling
            TsMessage::Pong(_) => todo!(),
            // TODO: handle receiving close frame from Guilded with reconnect
            TsMessage::Close(_frame) => todo!(),
            // Ignore Ping, tungstenite will automatically Pong
            TsMessage::Ping(_) => (),
            // Binary message should not be sent by Guilded
            TsMessage::Binary(_) => warn!("received binary message, ignoring"),
            // Frame should not be received per tungstenite docs
            TsMessage::Frame(_) => error!("received raw Message::Frame, ignoring"),
        }
    }
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
    async fn start(&mut self) -> Result<Events, ConnectingError> {
        let (tx, rx) = unbounded_channel();
        self.sender = Mutex::new(Some(tx));
        let sender = self
            .sender
            .lock()
            .expect("sender poisoned")
            .take()
            .expect("already started"); // TODO: handle already started -- take() will return None

        let (ptx, _prx) = unbounded_channel();
        let mut processor = Processor::new(&self.token, sender, ptx).await?;
        tokio::spawn(async move {
            processor.run().await;
        });
        Ok(rx)
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Set up Websocket Client for Guilded
    let token = std::env::var("GUILDED_BEARER_TOKEN")
        .expect("GUILDED_BEARER_TOKEN env var must be provided");
    let mut client = Client::new(&token).await?;
    let mut events = client.start().await?;

    // Set up Scryfall API HTTP client
    use mtg::http::Client as HttpClient;
    let http_client = HttpClient::new(&token);
    let re =
        Arc::new(Regex::new(r"\[\[(?P<query>.*?)\]\]").expect("failed to create message regex"));

    loop {
        if let Some(Event::ChatMessageCreated { message, .. }) = events.recv().await {
            let http_client = http_client.clone();
            let re = re.clone();
            tokio::spawn(async move {
                let query = re.captures(&message.content);
                if query.is_none() {
                    return;
                }
                let query = query.unwrap().name("query").unwrap().as_str();
                match mtg::scryfall::find(query).await {
                    Ok(Some(card)) => {
                        debug!(?card.name, "found card");
                        let res = http_client
                            .create_message(&message.channel_id, &card.into())
                            .await;
                        match res {
                            Ok(_) => debug!("create_message success"),
                            Err(err) => error!(?err, "create_message failed"),
                        }
                    }
                    Ok(None) => {
                        debug!(query, "found no matching card")
                    }
                    Err(err) => debug!(?err),
                }
            });
        }
    }
}
