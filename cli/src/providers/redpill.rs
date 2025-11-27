use super::Provider;
use serde_json::{json, Value};

/// Fallback provider using OpenAI-compatible API format (the default)
#[derive(Debug, Clone, Default)]
pub struct Redpill;

impl Redpill {
    /// Maximum tokens allowed for chat completion (required by Anthropic)
    const MAX_TOKENS: u32 = 2048;
}

impl Provider for Redpill {
    fn build_chat_body(&self, model_id: &str, messages: &[Value]) -> Value {
        // Check if the model is from Anthropic to adjust the max_tokens parameter
        if model_id.to_lowercase().contains("claude") {
            return json!({
                "model": model_id,
                "max_tokens": Self::MAX_TOKENS,
                "messages": messages
            });
        }

        json!({
            "model": model_id,
            "messages": messages
        })
    }

    fn models_headers(&self, _api_key: &str) -> Vec<(&'static str, String)> {
        vec![]
    }

    /// Response headers to censor for privacy (default: common tracking headers)
    fn response_censor_headers(&self) -> &'static [&'static str] {
        &["date", "cf-ray", "x-request-id", "set-cookie"]
    }
}
