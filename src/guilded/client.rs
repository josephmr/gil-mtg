use futures_util::StreamExt;
use std::{sync::Mutex, time::Duration};
use thiserror::Error;
use tokio::{
    net::TcpStream,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tokio_tungstenite::{
    connect_async, tungstenite::client::IntoClientRequest, tungstenite::Message as TsMessage,
    MaybeTlsStream, WebSocketStream,
};
use tracing::{debug, error, warn};

use crate::guilded::{Event, Message, Op, OpCode};

#[derive(Error, Debug)]
pub enum ConnectingError {
    #[error("invalid Guilded bearer token")]
    InvalidToken,
    #[error("error establishing websocket connection")]
    Establishing(#[from] tokio_tungstenite::tungstenite::Error),
}

pub struct Client {
    sender: Mutex<Option<UnboundedSender<Event>>>,
    token: String,
}

#[derive(Debug)]
enum ProcessorAction {}

struct Processor {
    token: String,
    events: UnboundedSender<Event>,
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

#[derive(Error, Debug)]
enum HandlingError {
    #[error("received close frame")]
    Close,
}

impl HandlingError {
    const fn reconnectable(&self) -> bool {
        match self {
            HandlingError::Close => true,
        }
    }
}

impl Processor {
    async fn new(token: &str, events: UnboundedSender<Event>) -> Result<Self, ConnectingError> {
        let ws_stream = Processor::connect(token).await?;
        Ok(Processor {
            token: token.to_string(),
            events,
            ws_stream,
        })
    }

    async fn connect(
        token: &str,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, ConnectingError> {
        let mut req = "wss://api.guilded.gg/v1/websocket"
            .into_client_request()
            .expect("failed to parse websocker uri");
        req.headers_mut().append(
            "Authorization",
            format!("Bearer {}", token)
                .parse()
                .map_err(|_| ConnectingError::InvalidToken)?,
        );
        debug!("processor attempting to connect to Guilded websocket");
        let (ws_stream, _) = connect_async(req).await?;
        debug!("processor websocket connected successfully");
        Ok(ws_stream)
    }

    async fn run(&mut self) {
        while let Some(message) = self.ws_stream.next().await {
            if let Err(err) = message {
                warn!(?err, "run received error getting next message from stream");
                continue;
            }

            if let Err(err) = self.handle_message(message.unwrap()) {
                if err.reconnectable() {
                    self.reconnect().await;
                    continue;
                }
            }
        }

        // Received a none, this should be interpreted as a force close of the
        // socket and attempt a reconnect
        error!("processor stream ended unexpectedly")
    }

    async fn reconnect(&mut self) {
        debug!("attempting to reconnect");
        let mut wait = Duration::from_secs(1);
        loop {
            tokio::time::sleep(wait).await;
            match Processor::connect(&self.token).await {
                Ok(ws_stream) => {
                    debug!("reconnection successful");
                    self.ws_stream = ws_stream;
                }
                Err(err) => {
                    warn!(?err, "failed to reconnect");
                    if wait < Duration::from_secs(123) {
                        wait *= 2
                    }
                    continue;
                }
            }
        }
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

    fn handle_message(&self, message: TsMessage) -> Result<(), HandlingError> {
        match message {
            TsMessage::Text(text) => self.handle_event(text),
            // TODO: implement client side heartbeats + pong handling
            TsMessage::Pong(_) => {
                warn!("received a pong without sending a ping");
            }
            // TODO: handle receiving close frame from Guilded with reconnect
            TsMessage::Close(frame) => {
                warn!(?frame, "received close frame");
                return Err(HandlingError::Close);
            }
            // Ignore Ping, tungstenite will automatically Pong
            TsMessage::Ping(_) => (),
            // Binary message should not be sent by Guilded
            TsMessage::Binary(_) => warn!("received binary message, ignoring"),
            // Frame should not be received per tungstenite docs
            TsMessage::Frame(_) => error!("received raw Message::Frame, ignoring"),
        }

        Ok(())
    }
}

impl Client {
    pub fn new(token: &str) -> Client {
        Client {
            sender: Mutex::new(None),
            token: token.to_string(),
        }
    }

    // TODO: Use type state to manage connection state (connected|connecting|not_connected)
    pub async fn start(&mut self) -> Result<UnboundedReceiver<Event>, ConnectingError> {
        let (tx, rx) = unbounded_channel();
        self.sender = Mutex::new(Some(tx));
        let sender = self
            .sender
            .lock()
            .expect("sender poisoned")
            .take()
            .expect("already started"); // TODO: handle already started -- take() will return None

        let mut processor = Processor::new(&self.token, sender).await?;
        tokio::spawn(async move {
            processor.run().await;
        });
        Ok(rx)
    }
}
