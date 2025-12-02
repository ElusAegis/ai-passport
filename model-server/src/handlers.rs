//! HTTP request handlers.

use crate::response::{extract_word_count, fixed_reply, generate_response};
use crate::types::{ChatChoice, ChatMessage, ChatRequest, ChatResponse, Model, ModelList, Usage};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use std::sync::Arc;
use time::OffsetDateTime;
use tracing::debug;
use uuid::Uuid;

/// Application state shared across handlers.
#[derive(Clone)]
pub struct AppState {
    pub models: Arc<Vec<Model>>,
}

/// GET /v1/models - List available models.
pub async fn list_models(State(state): State<AppState>) -> Json<ModelList> {
    Json(ModelList {
        object: "list",
        data: state.models.to_vec(),
    })
}

/// POST /v1/chat/completions - Generate a chat completion.
pub async fn chat_completions(
    Json(req): Json<ChatRequest>,
) -> (StatusCode, HeaderMap, Json<ChatResponse>) {
    let created = OffsetDateTime::now_utc().unix_timestamp();

    // Get the last user message
    let last_user_msg = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .unwrap_or("<none>");

    debug!(
        model = %req.model,
        history_len = req.messages.len(),
        max_tokens = ?req.max_tokens,
        "request: {}",
        &last_user_msg[..last_user_msg.len().min(70)]
    );

    // Simulate processing time based on model
    let sleep_duration_ms = match req.model.as_str() {
        "demo-gpt-4.5" => 2000,
        "demo-gpt-4o-mini" => 1000,
        "demo-gpt-3.5-turbo" => 700,
        _ => 10,
    };

    let sleep_duration = sleep_duration_ms + (rand::random::<u64>() % (sleep_duration_ms / 2));
    tokio::time::sleep(std::time::Duration::from_millis(sleep_duration)).await;

    // Generate response based on word count request or use fixed reply
    let content = if let Some(word_count) = extract_word_count(last_user_msg) {
        debug!(
            "Generating response with {} words (max_tokens: {:?})",
            word_count, req.max_tokens
        );
        generate_response(word_count, req.max_tokens)
    } else {
        fixed_reply(&req.model, last_user_msg)
    };

    let completion_tokens = (content.len() / 4) as u32; // Rough estimate
    let prompt_tokens = req
        .messages
        .iter()
        .map(|m| m.content.len() / 4)
        .sum::<usize>() as u32;

    debug!(
        "reply length: {} bytes, {} words",
        content.len(),
        content.split_whitespace().count()
    );

    let resp = ChatResponse {
        id: format!("chatcmpl-{}", Uuid::new_v4()),
        object: "chat.completion",
        created,
        model: req.model.clone(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".into(),
                content,
            },
            finish_reason: "stop",
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    };

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("OpenAI-Model", req.model.parse().unwrap());
    (StatusCode::OK, headers, Json(resp))
}
