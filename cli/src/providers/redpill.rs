use super::{Anthropic, ChatMessage, Provider};
use serde_json::{json, Value};

/// Fallback provider using OpenAI-compatible API format (the default)
#[derive(Debug, Clone, Default)]
pub struct Redpill;

impl Provider for Redpill {
    fn build_chat_body(
        &self,
        model_id: &str,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> Value {
        let mut body = json!({
            "model": model_id,
            "messages": messages
        });

        // Check if we need to enforce max_tokens (required for Claude models)
        if model_id.to_lowercase().contains("claude") || max_tokens.is_some() {
            let tokens = max_tokens.unwrap_or(Anthropic::MAX_TOKENS);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("max_tokens".to_string(), json!(tokens));
            }
        }

        body
    }

    fn models_headers_with_key(&self, _api_key: &str) -> Vec<(&'static str, String)> {
        vec![]
    }

    /// Response headers to censor for privacy (default: common tracking headers)
    fn response_censor_headers(&self) -> &'static [&'static str] {
        &["date", "cf-ray", "x-request-id", "set-cookie"]
    }
}
