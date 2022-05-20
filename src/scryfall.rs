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
pub enum SearchError {
    #[error("No matches found for card name {name}")]
    NoMatch { name: String },
    #[error("Scryfall API call failed")]
    HttpError(#[from] reqwest::Error),
}

// TODO: Fix API to make user create a client instead of global
static CLIENT: OnceCell<reqwest::Client> = OnceCell::new();

// TODO: Refactor to Result<Option<Card>, anyhow::Error>
pub async fn search(name: String) -> Result<Card, SearchError> {
    let client = CLIENT.get_or_init(reqwest::Client::new);
    let url = format!("https://api.scryfall.com/cards/search?q={}", name);
    let response = client
        .get(url)
        .send()
        .await?
        .json::<SearchResponse>()
        .await?;

    response
        .data
        .ok_or(SearchError::NoMatch { name })
        .map(|mut data| data.swap_remove(0))
}
