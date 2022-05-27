pub mod client;

use serde::{Deserialize, Serialize};
use serde_repr::*;

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
pub enum Event {
    #[serde(rename_all = "camelCase")]
    ChatMessageCreated {
        server_id: String,
        message: ChatMessage,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub channel_id: String,
    pub content: String,
}
