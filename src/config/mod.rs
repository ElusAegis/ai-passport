use crate::args::NotaryMode;
use crate::args::{ProveArgs, SessionMode, VerifyArgs};
use crate::config::load_api_domain::load_api_domain;
use crate::config::load_api_key::load_api_key;
use crate::config::load_api_port::load_api_port;
use crate::config::select_model::select_model_id;
use crate::config::select_proof_path::select_proof_path;
use crate::prove::setup::get_total_sent_recv_max;
use anyhow::{Context, Result};
use derive_builder::Builder;
use dialoguer::console::style;
use std::path::PathBuf;
use tlsn_common::config::NetworkSetting;

mod load_api_domain;
mod load_api_key;
mod load_api_port;
mod select_model;
mod select_proof_path;

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

#[derive(Builder, Clone)]
pub struct ModelConfig {
    /// The domain of the server hosting the model API
    pub(crate) domain: String,
    /// The port of the server hosting the model API
    #[builder(setter(into), default = "443")]
    pub(crate) port: u16,
    /// The route for inference requests
    #[builder(setter(into), default = "String::from(\"/v1/chat/completions\")")]
    pub(crate) inference_route: String,
    /// The route for listing available models
    #[builder(setter(into), default = "String::from(\"/v1/models\")")]
    pub(crate) model_list_route: String,
    /// The API key for authentication with the model API
    #[builder(setter(into))]
    pub(crate) api_key: String,
    /// The ID of the model
    #[builder(setter(into))]
    pub model_id: String,
}

impl ModelConfig {
    pub fn builder() -> ModelConfigBuilder {
        ModelConfigBuilder::default()
    }
}

#[derive(Builder, Clone)]
pub struct NotaryConfig {
    /// The domain of the notary server
    pub(crate) domain: String,
    /// The port of the notary server
    #[builder(setter(into))]
    pub(crate) port: u16,
    /// The route for notary requests
    #[builder(setter(into))]
    pub(crate) path_prefix: String,
    /// Notary type
    #[builder(default = "NotaryMode::Ephemeral")]
    pub(crate) mode: NotaryMode,
}

impl NotaryConfig {
    pub fn builder() -> NotaryConfigBuilder {
        NotaryConfigBuilder::default()
    }
}

#[derive(Builder, Clone)]
pub struct NotarisationConfig {
    /// Notary configuration
    pub(crate) notary_config: NotaryConfig,
    /// Maximum expected number of requests to send
    pub(crate) max_req_num_sent: usize,
    /// Maximum number of bytes in a user prompt
    pub(crate) max_single_request_size: usize,
    /// Maximum number of bytes in the response
    pub(crate) max_single_response_size: usize,
    /// Network optimization strategy
    #[builder(default)]
    pub(crate) network_optimization: NetworkSetting,
    /// Two modes:
    /// - **One‑shot**: we spin up a fresh protocol instance per request/response pair,
    ///   so we can size the send/recv budgets exactly from the *current* message sizes.
    /// - **Multi‑round (default)**: the model API is stateless; every new request must
    ///   re‑send the whole conversation so far. That makes request sizes grow roughly
    ///   quadratically with the number of rounds.
    #[builder(default)]
    pub(crate) mode: SessionMode,
}

impl NotarisationConfig {
    pub fn builder() -> NotarisationConfigBuilder {
        NotarisationConfigBuilder::default()
    }
}

#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct ProveConfig {
    pub(crate) model_config: ModelConfig,
    #[builder(default)]
    pub(crate) privacy_config: PrivacyConfig,
    pub(crate) notarisation_config: NotarisationConfig,
}

impl ProveConfig {
    pub fn builder() -> ProveConfigBuilder {
        ProveConfigBuilder::default()
    }

    pub(crate) async fn setup(args: ProveArgs) -> Result<ProveConfig> {
        let _ = dotenvy::from_filename(args.env_file);

        let api_domain = load_api_domain().context("Failed to load API domain")?;
        let api_key = load_api_key().context("Failed to load API key")?;
        let api_port = load_api_port().context("Failed to load API port")?;

        let mut model_config_builder = ModelConfig::builder()
            .api_key(api_key)
            .domain(api_domain)
            .port(api_port)
            .clone();

        let model_id = match args.model_id {
            Some(id) => id,
            None => select_model_id(&model_config_builder.model_id("tmp").build()?)
                .await
                .context("Failed to select model")?,
        };

        let model_config = model_config_builder
            .model_id(model_id)
            .build()
            .context("Failed to build model")?;

        let notary_config = NotaryConfig::builder()
            .domain(args.notary_domain)
            .mode(args.notary_mode)
            .path_prefix(args.notary_version)
            .port(args.notary_port)
            .build()
            .context("Failed to build Notary Config")?;

        let notarisation_config = NotarisationConfig::builder()
            .notary_config(notary_config)
            .max_req_num_sent(args.max_req_num_sent)
            .max_single_request_size(args.max_single_request_size)
            .max_single_response_size(args.max_single_response_size)
            .mode(args.session_mode)
            .network_optimization(args.network_optimization)
            .build()?;

        let config: Self = Self::builder()
            .model_config(model_config)
            .notarisation_config(notarisation_config)
            .build()?;

        Self::print_config_summary(&config)?;

        Ok(config)
    }

    fn print_config_summary(config: &ProveConfig) -> Result<()> {
        // --- small helpers -------------------------------------------------------
        let check = || style("✔").green().bold();

        let kv = |k: &str, v: String| {
            format!(
                "{} {} {}",
                check(),
                style(k).bold(),
                style(format!("· {}", v)).dim()
            )
        };

        let fmt_kb_1 = |bytes: usize| format!("{:.1} KB", bytes as f64 / 1024.0);
        let est_tokens = |bytes: usize| bytes / 5;

        // Normalize routes so the print is consistent
        let norm_route = |r: &str| {
            if r.starts_with('/') {
                r.to_string()
            } else {
                format!("/{}", r)
            }
        };

        // --- Model API -----------------------------------------------------------
        println!(
            "{}",
            kv(
                "Model Inference API",
                format!(
                    "{}:{}{}",
                    config.model_config.domain,
                    config.model_config.port,
                    norm_route(&config.model_config.inference_route),
                ),
            )
        );

        println!("{}", kv("Model ID", config.model_config.model_id.clone()));

        // --- Notary --------------------------------------------------------------
        println!(
            "{}",
            kv(
                "Notary API",
                format!(
                    "{}:{}/{}",
                    config.notarisation_config.notary_config.domain,
                    config.notarisation_config.notary_config.port,
                    config
                        .notarisation_config
                        .notary_config
                        .path_prefix
                        .trim_start_matches('/'),
                ),
            )
        );

        println!(
            "{}",
            kv(
                "Notary Mode",
                format!("{:?}", config.notarisation_config.notary_config.mode),
            )
        );

        // --- Protocol -------------------------------------------------------------
        let s_req = config.notarisation_config.max_single_request_size;
        let s_res = config.notarisation_config.max_single_response_size;
        let (total_sent, total_recv) = get_total_sent_recv_max(&config.notarisation_config);

        println!(
            "{}",
            kv(
                "Protocol Session Mode",
                format!("{}", config.notarisation_config.mode),
            )
        );

        println!(
            "{}",
            kv(
                "Max Number of Model Requests",
                format!("{}", config.notarisation_config.max_req_num_sent),
            )
        );

        println!(
            "{}",
            kv(
                "Max Single Request Size",
                format!(
                    "{} (~{} tokens | total {})",
                    fmt_kb_1(s_req),
                    est_tokens(s_req),
                    fmt_kb_1(total_sent),
                ),
            )
        );

        println!(
            "{}",
            kv(
                "Max Single Response Size",
                format!(
                    "{} (~{} tokens | total {})",
                    fmt_kb_1(s_res),
                    est_tokens(s_res),
                    fmt_kb_1(total_recv),
                ),
            )
        );

        // --- Footer --------------------------------------------------------------
        println!(
            "{} {} {}\n\n",
            style("✔").blue(),
            style("Configuration complete").bold(),
            style("✔").blue()
        );

        Ok(())
    }
}

#[derive(Builder)]
pub struct VerifyConfig {
    pub(crate) proof_path: PathBuf,
}

impl VerifyConfig {
    pub(crate) fn builder() -> VerifyConfigBuilder {
        VerifyConfigBuilder::default()
    }

    pub(crate) fn setup(args: VerifyArgs) -> Result<VerifyConfig> {
        let raw_path = match args.proof_path {
            Some(path) => path,
            None => select_proof_path()?,
        };

        // Prefer a canonical absolute path if possible
        let path = PathBuf::from(raw_path);
        let path = std::fs::canonicalize(&path).unwrap_or(path);

        Self::builder().proof_path(path).build().map_err(Into::into)
    }
}
