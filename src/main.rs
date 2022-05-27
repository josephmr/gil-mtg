use mtg::guilded::client::Client;
use mtg::guilded::Event;
use regex::Regex;
use std::sync::Arc;
use tracing::{debug, error};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Set up Websocket Client for Guilded
    let token = std::env::var("GUILDED_BEARER_TOKEN")
        .expect("GUILDED_BEARER_TOKEN env var must be provided");
    let mut client = Client::new(&token);
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
