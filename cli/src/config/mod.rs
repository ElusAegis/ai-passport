use crate::args::{ProveArgs, VerifyArgs};
use crate::config::{
    load::{
        api_domain::load_api_domain, api_key::load_api_key, api_port::load_api_port,
        model_id::load_model_id, proof_path::load_proof_path,
    },
    notary::NotaryConfig,
    privacy::PrivacyConfig,
};
use anyhow::{Context, Result};
use derive_builder::Builder;
use dialoguer::console::style;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

mod load;
mod model;
pub mod notary;
pub mod privacy;

/// Expected byte package overhead for a single request.
const REQUEST_OVERHEAD: usize = 700;
/// Expected byte package overhead for a single response.
const RESPONSE_OVERHEAD: usize = 700;

#[derive(Builder, Clone)]
pub struct ServerConfig {
    /// The domain of the server hosting the model API
    pub(crate) domain: String,
    /// The port of the server hosting the model API
    #[builder(setter(into), default = "443")]
    pub(crate) port: u16,
}

impl ServerConfig {
    pub fn builder() -> ServerConfigBuilder {
        ServerConfigBuilder::default()
    }
}

#[derive(Builder, Clone)]
pub struct ModelConfig {
    pub(crate) server: ServerConfig,
    /// The route for inference requests
    #[builder(setter(into), default = "String::from(\"/v1/chat/completions\")")]
    pub(crate) inference_route: String,
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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub enum SessionMode {
    /// Create a fresh protocol instance per request/response pair.
    #[default]
    Multi,
    /// Keep a single protocol instance across multiple requests (stateless API -> resend history).
    Single,
}

#[derive(Builder, Clone)]
pub struct SessionConfig {
    /// Maximum expected number of requests to the model
    pub(crate) max_msg_num: usize,
    /// Maximum number of bytes in a user prompt
    pub max_single_request_size: usize,
    /// Maximum number of bytes in the response
    pub max_single_response_size: usize,
    /// Maximum number of bytes in the whole request (including package overhead)
    #[builder(default = self.max_single_request_size.unwrap() + REQUEST_OVERHEAD)]
    pub max_total_single_request_size: usize,
    /// Maximum number of bytes in the whole response (including package overhead)
    #[builder(default = self.max_single_response_size.unwrap() + RESPONSE_OVERHEAD)]
    pub max_total_response_size: usize,
    /// Two modes:
    /// - **One‑shot**: we spin up a fresh protocol instance per request/response pair,
    ///   so we can size the send/recv budgets exactly from the *current* message sizes.
    /// - **Multi‑round (default)**: the model API is stateless; every new request must
    ///   re‑send the whole conversation so far. That makes request sizes grow roughly
    ///   quadratically with the number of rounds.
    #[builder(default)]
    pub(crate) mode: SessionMode,
}

impl SessionConfig {
    pub fn builder() -> SessionConfigBuilder {
        SessionConfigBuilder::default()
    }

    pub fn max_total_sent_recv(&self) -> (usize, usize) {
        let n = self.max_msg_num;

        let req = self.max_single_request_size;
        let rsp = self.max_single_response_size;

        let full_req = self.max_total_single_request_size;
        let full_rsp = self.max_total_response_size;

        if matches!(self.mode, SessionMode::Multi) {
            let max_total_sent = full_req + (n - 1) * (req + rsp);
            let max_total_recv = full_rsp;

            (max_total_sent, max_total_recv)
        } else {
            let max_total_sent = full_req * n + n * (n - 1) * (req + rsp) / 2;
            let max_total_recv = full_rsp * n;

            (max_total_sent, max_total_recv)
        }
    }
}

#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct ProveConfig {
    pub(crate) model: ModelConfig,
    #[builder(default)]
    pub(crate) privacy: PrivacyConfig,
    pub notary: NotaryConfig,
    pub session: SessionConfig,
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
        let model_list_route = args.model_list_route;

        let server_config = ServerConfig::builder()
            .domain(api_domain.clone())
            .port(api_port)
            .build()
            .context("Failed to build server configuration")?;

        let model_id = match args.model_id {
            Some(id) => id,
            None => load_model_id(&server_config, &model_list_route)
                .await
                .context("Failed to select model")?,
        };

        let model_config = ModelConfig::builder()
            .server(server_config)
            .api_key(api_key)
            .model_id(model_id)
            .build()
            .context("Failed to build model")?;

        let session_config = SessionConfig::builder()
            .max_msg_num(args.max_msg_num)
            .max_single_request_size(args.max_single_request_size)
            .max_single_response_size(args.max_single_response_size)
            .mode(args.session_mode)
            .build()
            .context("Failed to build session configuration")?;

        let notary_config = NotaryConfig::builder()
            .domain(args.notary_domain)
            .mode(args.notary_mode)
            .path_prefix(args.notary_version)
            .port(args.notary_port)
            .network_optimization(args.network_optimization)
            .finalize_for_session(&session_config)?;

        let config: Self = Self::builder()
            .model(model_config)
            .privacy(PrivacyConfig::default())
            .session(session_config)
            .notary(notary_config)
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

        // --- Model API -----------------------------------------------------------
        info!(target: "plain",
            "{}",
            kv(
                "Model Inference API",
                format!(
                    "{}:{}/{}",
                    config.model.server.domain,
                    config.model.server.port,
                    config
                        .model
                        .inference_route
                        .trim_start_matches('/')
                ),
            )
        );

        info!(target: "plain", "{}", kv("Model ID", config.model.model_id.clone()));

        // --- Notary --------------------------------------------------------------
        info!(target: "plain",
            "{}",
            kv(
                "Notary API",
                format!(
                    "{}:{}/{}",
                    config.notary.domain,
                    config.notary.port,
                    config
                        .notary
                        .path_prefix
                        .trim_start_matches('/')
                ),
            )
        );

        info!(target: "plain",
            "{}",
            kv(
                "Notary Mode",
                format!("{:?}", config.notary.mode),
            )
        );

        info!(target: "plain",
            "{}",
            kv(
                "Network Optimisation",
                format!("{:?}", config.notary.network_optimization),
            )
        );

        // --- Protocol ------------------------------------------------------------
        let s_req = config.session.max_single_request_size;
        let s_res = config.session.max_single_response_size;
        let total_sent = config.notary.max_total_sent;
        let total_recv = config.notary.max_total_recv;

        info!(target: "plain",
            "{}",
            kv(
                "Protocol Session Mode",
                format!("{}", config.session.mode),
            )
        );

        info!(target: "plain",
            "{}",
            kv(
                "Max Number of Model Requests",
                format!("{}", config.session.max_msg_num),
            )
        );

        info!(target: "plain",
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

        info!(target: "plain",
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
        info!(target: "plain",
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
    pub(crate) accept_key: bool,
}

impl VerifyConfig {
    pub(crate) fn builder() -> VerifyConfigBuilder {
        VerifyConfigBuilder::default()
    }

    pub(crate) fn setup(args: VerifyArgs) -> Result<VerifyConfig> {
        let raw_path = args.proof_path.unwrap_or(load_proof_path()?);

        // Prefer a canonical absolute path if possible
        let path = PathBuf::from(raw_path);
        let path = std::fs::canonicalize(&path).unwrap_or(path);

        // Consistent, concise summary line
        info!(target: "plain",
            "{} {} {}",
            style("✔").green(),
            style("Selected proof path").bold(),
            style(path.display().to_string()).dim()
        );

        Self::builder()
            .proof_path(path)
            .accept_key(args.accept_key)
            .build()
            .map_err(Into::into)
    }
}
