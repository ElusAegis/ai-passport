use derive_builder::Builder;

/// Privacy settings including topics to censor in requests and responses
#[derive(Builder, Clone)]
pub struct PrivacyConfig {
    pub(crate) request_topics_to_censor: &'static [&'static str],
    pub(crate) response_topics_to_censor: &'static [&'static str],
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            request_topics_to_censor: &["authorization"],
            response_topics_to_censor: &[
                "anthropic-ratelimit-requests-reset",
                "anthropic-ratelimit-tokens-reset",
                "request-id",
                "x-kong-request-id",
                "cf-ray",
                "server-timing",
                "report-to",
            ],
        }
    }
}
