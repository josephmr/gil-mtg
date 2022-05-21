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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<ChatEmbedFooter>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ChatEmbedFooter {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<ChatEmbed>>,
}

impl From<crate::scryfall::Card> for Message {
    fn from(card: crate::scryfall::Card) -> Self {
        Message {
            embeds: Some(vec![ChatEmbed {
                title: Some(card.name),
                url: Some(card.scryfall_uri),
                image: Some(UrlWrapper {
                    url: card.image_uris.map(|c| c.small),
                }),
                footer: Some(ChatEmbedFooter {
                    text: "powered by Scryfall API".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }]),
            ..Default::default()
        }
    }
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
        channel_id: &String,
        message: &Message,
    ) -> Result<(), CreateError> {
        let json = serde_json::json!(&message);
        let resp = self
            .client
            .post(format!(
                "https://www.guilded.gg/api/v1/channels/{}/messages",
                channel_id
            ))
            .header("Authorization", format!("Bearer {}", &self.token))
            .json(message)
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
