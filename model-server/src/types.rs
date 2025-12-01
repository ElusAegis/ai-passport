//! API types for OpenAI-compatible chat completions.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Model information returned by the /v1/models endpoint.
#[derive(Serialize, Clone)]
pub struct Model {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub owned_by: &'static str,
}

/// List of models.
#[derive(Serialize)]
pub struct ModelList {
    pub object: &'static str,
    pub data: Vec<Model>,
}

/// A single chat message.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Chat completion request.
#[derive(Deserialize, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// Maximum number of tokens to generate.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    // Other optional OpenAI fields (temperature, top_p, stream, etc.) - ignored for now
}

/// A single choice in the chat completion response.
#[derive(Serialize)]
pub struct ChatChoice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: &'static str,
}

/// Token usage statistics.
#[derive(Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Chat completion response.
#[derive(Serialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    pub usage: Usage,
}

/// Get the list of available demo models.
pub fn demo_models() -> Vec<Model> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    vec![
        Model {
            id: "demo-gpt-4o-mini".into(),
            object: "model",
            created: now,
            owned_by: "demo",
        },
        Model {
            id: "demo-gpt-3.5-turbo".into(),
            object: "model",
            created: now,
            owned_by: "demo",
        },
    ]
}