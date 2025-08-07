use crate::remote::attribution::config::model_selection::select_model_id;
use anyhow::{Context, Result};
use derive_builder::Builder;
use load_api_key::load_api_key;

mod load_api_key;
mod model_selection;

/// Configuration for API settings, including server endpoints and the API key
#[derive(Debug, Default)]
pub struct ModelApiSettings {
    pub server_domain: &'static str,
    pub inference_route: &'static str,
    pub model_list_route: &'static str,
    pub api_key: String,
}

impl ModelApiSettings {
    fn new(api_key: String) -> Self {
        Self {
            server_domain: "api.red-pill.ai",
            inference_route: "/v1/chat/completions",
            model_list_route: "/v1/models",
            api_key,
        }
    }
}

#[derive(Debug)]
pub struct NotarySettings {
    pub host: &'static str,
    pub port: u16,
    pub path: &'static str,
    pub enable_tls: bool,
}

/// Configuration for Notary settings, defining host, port, and path
impl Default for NotarySettings {
    fn default() -> Self {
        NotarySettings {
            host: "notary.pse.dev",
            port: 443,
            path: "v0.1.0-alpha.12",
            enable_tls: true,
        }
    }
}

impl NotarySettings {
    fn local() -> Self {
        NotarySettings {
            host: "localhost",
            port: 7047,
            path: "",
            enable_tls: false,
        }
    }
}

/// Privacy settings including topics to censor in requests and responses
#[derive(Debug)]
#[allow(dead_code)]
pub struct PrivacySettings {
    pub request_topics_to_censor: &'static [&'static str],
    pub response_topics_to_censor: &'static [&'static str],
}

impl Default for PrivacySettings {
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

/// Model settings including API settings, model ID, and setup prompt
#[derive(Debug)]
pub struct ModelSettings {
    pub api_settings: ModelApiSettings,
    pub id: String,
}

impl ModelSettings {
    fn new(model_id: String, api_settings: ModelApiSettings) -> Self {
        Self {
            api_settings,
            id: model_id,
        }
    }
}

/// Complete application configuration including model, privacy, and notary settings
#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub(crate) struct ApplicationConfig {
    pub(crate) model_settings: ModelSettings,
    #[builder(default)]
    #[allow(dead_code)]
    pub(crate) privacy_settings: PrivacySettings,
    #[builder(default)]
    pub(crate) notary_settings: NotarySettings,
}

/// Setup configuration by loading API key, selecting a model, and returning Config
pub(crate) async fn setup_config() -> Result<ApplicationConfig> {
    let api_key = load_api_key().context("Failed to load API key")?;
    let api_settings = ModelApiSettings::new(api_key.clone());

    let model_id = select_model_id(&api_settings)
        .await
        .context("Failed to select model")?;

    let model_settings = ModelSettings::new(model_id, api_settings);

    // Check if the LOCAL_NOTARY environment variable is set
    let notary_settings = if std::env::var("LOCAL_NOTARY").is_ok() {
        NotarySettings::local()
    } else {
        NotarySettings::default()
    };

    Ok(ApplicationConfigBuilder::default()
        .model_settings(model_settings)
        .notary_settings(notary_settings)
        .build()?)
}
