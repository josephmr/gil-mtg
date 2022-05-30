/// test_ping
///
/// A simple binary used to test whether tokio_tungstenite automatically responds to ping frames... It does.
///
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{debug, warn};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();
    debug!("connecting...");
    let (mut ws_stream, _) = connect_async("wss://SeriousAdmiredAtoms.josephmr.repl.co").await?;
    while let Some(message) = ws_stream.next().await {
        match message.unwrap() {
            WsMessage::Ping(_) => debug!("ping!"),
            WsMessage::Pong(_) => debug!("pong!"),
            WsMessage::Text(text) => debug!(?text),
            _ => warn!("unknown!"),
        }
    }
    Ok(())
}
