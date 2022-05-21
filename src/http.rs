use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

#[derive(Serialize, Deserialize, Default)]
pub struct UrlWrapper {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ChatEmbed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<UrlWrapper>,
}

// TODO: impl From<mtg::scryfall::Card>
#[derive(Serialize, Deserialize, Default)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<ChatEmbed>>,
}

#[derive(Error, Debug)]
pub enum CreateError {
    #[error("Guilded API call returned status {0}: {1}")]
    Failed(reqwest::StatusCode, String),
    #[error("Guilded API call failed")]
    HttpError(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct Client {
    token: String,
    client: reqwest::Client,
}

impl Client {
    pub fn new(token: &str) -> Client {
        Client {
            token: token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    // TODO: create Id types for ChannelId
    pub async fn create_message(
        &self,
        channel_id: String,
        message: Message,
    ) -> Result<(), CreateError> {
        let json = serde_json::json!(&message);
        let resp = self
            .client
            .post(format!(
                "https://www.guilded.gg/api/v1/channels/{}/messages",
                channel_id
            ))
            .header("Authorization", format!("Bearer {}", &self.token))
            .json(&message)
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::CREATED {
            debug!(?json);
            return Err(CreateError::Failed(
                resp.status(),
                resp.text().await.unwrap_or_default(),
            ));
        }
        Ok(())
    }
}
