use futures_util::StreamExt;
use std::sync::Mutex;
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

enum ProcessorAction {}

struct Processor {
    events: UnboundedSender<Event>,
    _controls: UnboundedSender<ProcessorAction>,
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl Processor {
    async fn new(
        token: &str,
        events: UnboundedSender<Event>,
        _controls: UnboundedSender<ProcessorAction>,
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
            _controls,
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

        let (ptx, _prx) = unbounded_channel();
        let mut processor = Processor::new(&self.token, sender, ptx).await?;
        tokio::spawn(async move {
            processor.run().await;
        });
        Ok(rx)
    }
}
