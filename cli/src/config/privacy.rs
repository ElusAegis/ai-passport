use crate::providers::Provider;

/// Privacy settings including topics to censor in requests and responses
#[derive(Clone)]
pub struct PrivacyConfig {
    pub(crate) request_topics_to_censor: &'static [&'static str],
    pub(crate) response_topics_to_censor: &'static [&'static str],
}

impl<T: Provider> From<T> for PrivacyConfig
where
    T: Provider,
{
    fn from(provider: T) -> Self {
        Self {
            request_topics_to_censor: provider.request_censor_headers(),
            response_topics_to_censor: provider.response_censor_headers(),
        }
    }
}
