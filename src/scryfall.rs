use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Deserialize)]
struct SearchResponse {
    data: Option<Vec<Card>>,
}

#[derive(Serialize, Deserialize)]
pub struct Card {
    pub name: String,
    pub image_uris: Option<ImageUris>,
}

#[derive(Serialize, Deserialize)]
pub struct ImageUris {
    pub small: String,
    pub normal: String,
    pub large: String,
}

#[derive(Error, Debug)]
pub enum ScryfallError {
    #[error("Scryfall API call failed")]
    HttpError(#[from] reqwest::Error),
    #[error("Scryfall API returned unrecognized status {0}: {1}")]
    UnknownResponse(reqwest::StatusCode, String),
}

// TODO: Fix API to make user create a client instead of global
static CLIENT: OnceCell<reqwest::Client> = OnceCell::new();

pub async fn search(query: &str) -> Result<Option<Vec<Card>>, ScryfallError> {
    let client = CLIENT.get_or_init(reqwest::Client::new);
    let url = format!("https://api.scryfall.com/cards/search?q={}", query);
    Ok(client
        .get(url)
        .send()
        .await?
        .json::<SearchResponse>()
        .await?
        .data)
}

pub async fn find(name: &str) -> Result<Option<Card>, ScryfallError> {
    let client = CLIENT.get_or_init(reqwest::Client::new);
    let url = format!("https://api.scryfall.com/cards/named?fuzzy={}", name);
    let response = client.get(url).send().await?;
    match response.status() {
        reqwest::StatusCode::OK => Ok(Some(response.json::<Card>().await?)),
        reqwest::StatusCode::NOT_FOUND => Ok(None),
        status => Err(ScryfallError::UnknownResponse(
            status,
            response
                .text()
                .await
                .unwrap_or("failed to get response body".to_string()),
        )),
    }
}
