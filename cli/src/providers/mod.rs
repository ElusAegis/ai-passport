mod anthropic;
mod mistral;
mod redpill;
mod unknown;

pub use anthropic::Anthropic;
pub use mistral::Mistral;
pub use redpill::Redpill;
pub use unknown::Unknown;

use enum_dispatch::enum_dispatch;
use serde_json::{json, Value};

#[enum_dispatch]
pub trait Provider {
    /// Endpoint path for chat/message completions (default: OpenAI-style)
    fn chat_endpoint(&self) -> &'static str {
        "/v1/chat/completions"
    }

    /// Provider-specific headers for chat/messages endpoint (default: Bearer token)
    fn chat_headers(&self, api_key: &str) -> Vec<(&'static str, String)> {
        vec![("Authorization", format!("Bearer {}", api_key))]
    }

    /// Build the request body for a chat completion (default: OpenAI-style)
    fn build_chat_body(&self, model_id: &str, messages: &[Value]) -> Value {
        json!({
            "model": model_id,
            "messages": messages
        })
    }

    /// Parse the assistant's content from the response (default: OpenAI-style)
    fn parse_chat_content<'a>(&self, response: &'a Value) -> Option<&'a str> {
        response["choices"][0]["message"]["content"].as_str()
    }

    /// Endpoint path for listing available models (default: OpenAI-style)
    fn models_endpoint(&self) -> &'static str {
        "/v1/models"
    }

    /// Provider-specific headers for models endpoint (default: none)
    fn models_headers(&self, _api_key: &str) -> Vec<(&'static str, String)> {
        vec![]
    }

    /// Request headers to censor for privacy (default: authorization)
    fn request_censor_headers(&self) -> &'static [&'static str] {
        &["authorization"]
    }

    /// Response headers to censor for privacy (default: common tracking headers)
    fn response_censor_headers(&self) -> &'static [&'static str];
}

#[enum_dispatch(Provider)]
#[derive(Debug, Clone)]
pub enum ApiProvider {
    Unknown,
    Anthropic,
    Mistral,
    Redpill,
}

impl ApiProvider {
    /// Detect the appropriate provider based on domain
    pub fn from_domain(domain: &str) -> Self {
        if domain.contains("anthropic") {
            Anthropic.into()
        } else if domain.contains("mistral") {
            Mistral.into()
        } else if domain.contains("red-pill") {
            Redpill.into()
        } else {
            Unknown.into()
        }
    }
}
