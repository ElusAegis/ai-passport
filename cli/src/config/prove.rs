use crate::cli::ProveArgs;
use crate::config::load::api_domain::load_api_domain;
use crate::config::load::api_key::load_api_key;
use crate::config::load::api_port::load_api_port;
use crate::config::load::model_id::load_model_id;
use crate::providers::Provider;
use crate::ApiProvider;
use anyhow::Context;
use derive_builder::Builder;
use dialoguer::console::style;
use tracing::info;

#[derive(Builder, Clone)]
pub struct ProveConfig {
    /// API Provider
    pub provider: ApiProvider,
    /// The ID of the model
    #[builder(setter(into))]
    pub model_id: String,
    /// Max bytes for the model response
    #[builder(setter(into), default)]
    pub max_response_bytes: Option<u32>,
    /// Max bytes for the model request
    #[builder(setter(into), default)]
    pub max_request_bytes: Option<u32>,
}

impl ProveConfigBuilder {}

impl ProveConfig {
    pub fn builder() -> ProveConfigBuilder {
        ProveConfigBuilder::default()
    }

    /// Load model configuration from args and environment
    pub(crate) async fn from_args(args: &ProveArgs) -> anyhow::Result<ProveConfig> {
        let _ = dotenvy::from_filename(&args.env_file);

        let api_domain = load_api_domain().context("Failed to load API domain")?;
        let api_key = load_api_key().context("Failed to load API key")?;
        let api_port = load_api_port().context("Failed to load API port")?;

        let api_provider = ApiProvider::builder()
            .domain(api_domain.clone())
            .port(api_port)
            .api_key(api_key.clone())
            .build()
            .context("Failed to build ApiProvider")?;

        let model_id = match &args.model_id {
            Some(id) => id.clone(),
            None => load_model_id(&api_provider)
                .await
                .context("Failed to select model")?,
        };

        if args.model_chat_route.is_some() {
            anyhow::bail!("Custom chat routes are not supported in this version");
        }

        if args.model_list_route.is_some() {
            anyhow::bail!("Custom model list routes are not supported in this version");
        }

        let config = Self::builder()
            .provider(api_provider)
            .model_id(model_id)
            .build()
            .context("Failed to build ProveConfig")?;

        Self::print_config_summary(&config)?;

        Ok(config)
    }

    fn print_config_summary(config: &ProveConfig) -> anyhow::Result<()> {
        let check = || style("✔").green().bold();

        let kv = |k: &str, v: &str| {
            format!(
                "{} {} {}",
                check(),
                style(k).bold(),
                style(format!("· {}", v)).dim()
            )
        };

        info!(target: "plain",
            "{}",
            kv(
                "Model Inference API",
                &format!(
                    "{}:{}{}",
                    config.provider.domain,
                    config.provider.port,
                    config.provider.chat_endpoint(),
                ),
            )
        );

        info!(target: "plain", "{}", kv("Model ID", &config.model_id));

        info!(target: "plain",
            "{} {} {}\n\n",
            style("✔").blue(),
            style("Configuration complete").bold(),
            style("✔").blue()
        );

        Ok(())
    }
}
