use crate::config::notary::NotaryMode;
use crate::SessionMode;
use clap::ValueHint;
use clap::{Args, Parser, Subcommand};
use std::fmt::Display;
use std::path::PathBuf;
use tlsn_common::config::NetworkSetting;

pub const DEFAULT_NETWORK_OPTIMIZATION: &str = "latency"; // parsed by parser
pub const DEFAULT_SESSION_MODE: &str = "single"; // parsed by parser

pub const DEFAULT_NOTARY_TYPE: &str = "remote"; // parsed by parser
pub const DEFAULT_NOTARY_DOMAIN: &str = "notary.pse.dev"; // default remote notary server
pub const DEFAULT_NOTARY_VERSION: &str = "v0.1.0-alpha.12"; // default notary version

pub const DEFAULT_MAX_REQ_NUM_SENT: usize = 3; // e.g., up to 3 model API calls
pub const DEFAULT_MAX_SINGLE_REQUEST_SIZE: usize = 1024; // 1 KiB prompt budget
pub const DEFAULT_MAX_SINGLE_RESPONSE_SIZE: usize = 1024; // 1 KiB response budget

impl Display for SessionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionMode::Multi => write!(f, "multi"),
            SessionMode::Single => write!(f, "single"),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Prove model interaction
    Prove(ProveArgs),

    /// Verify model interaction
    Verify(VerifyArgs),
}

#[derive(Args)]
pub(crate) struct ProveArgs {
    /// Specify the model to use (optional for proving)
    #[arg(long)]
    pub(crate) model_id: Option<String>,

    /// Path to environment file (default: ./.env). Can also use APP_ENV_FILE.
    #[arg(
        long,
        value_hint = ValueHint::FilePath,
        default_value = ".env",
        env = "APP_ENV_FILE",
        global = true
    )]
    pub(crate) env_file: PathBuf,

    /// Model API route to get a model list (optional - defaults to provider-specific endpoint)
    #[arg(long, env = "MODEL_LIST_ROUTE")]
    pub(crate) model_list_route: Option<String>,

    /// Model API route for chat/inference requests (optional - defaults to provider-specific endpoint)
    #[arg(long, env = "MODEL_CHAT_ROUTE")]
    pub(crate) model_chat_route: Option<String>,

    /// Maximum expected number of requests to send
    #[arg(
        long,
        env = "MAX_REQ_NUM_SENT",
        default_value_t = DEFAULT_MAX_REQ_NUM_SENT
    )]
    pub(crate) max_msg_num: usize,

    /// Maximum number of bytes in user prompt
    #[arg(
        long,
        env = "MAX_SINGLE_REQUEST_SIZE",
        default_value_t = DEFAULT_MAX_SINGLE_REQUEST_SIZE
    )]
    pub(crate) max_single_request_size: usize,

    /// Maximum number of bytes in the response
    #[arg(
        long,
        env = "MAX_SINGLE_RESPONSE_SIZE",
        default_value_t = DEFAULT_MAX_SINGLE_RESPONSE_SIZE
    )]
    pub(crate) max_single_response_size: usize,

    /// Network optimization strategy (latency | bandwidth).
    #[arg(
        long,
        env = "NETWORK_OPTIMIZATION",
        value_parser = parse_network_setting,
        default_value = DEFAULT_NETWORK_OPTIMIZATION
    )]
    pub(crate) network_optimization: NetworkSetting,

    /// Session mode (multi | single).
    /// multi: new protocol per request; exact per-round sizing.
    /// single: single protocol; resend growing history.
    #[arg(
        long,
        env = "SESSION_MODE",
        value_parser = parse_session_mode,
        default_value = DEFAULT_SESSION_MODE
    )]
    pub(crate) session_mode: SessionMode,

    /// Notary type (remote | ephemeral)
    /// remote (remote_tls): use a remote notary server with TLS.
    /// remote_non_tls: use a remote notary server without TLS.
    /// ephemeral: use an ephemeral notary server that spins up locally.
    #[arg(
        long,
        env = "NOTARY_TYPE",
        value_parser = parse_notary_type,
        default_value = DEFAULT_NOTARY_TYPE,
    )]
    pub(crate) notary_mode: NotaryMode,

    /// Notary server domain (optional for remote notary)
    /// If not provided, will use the default notary server `notary.pse.dev`.
    #[arg(
        long,
        env = "NOTARY_DOMAIN",
        value_hint = ValueHint::Hostname,
        default_value = DEFAULT_NOTARY_DOMAIN,
    )]
    pub(crate) notary_domain: String,

    /// Notary version (optional for remote notary)
    #[arg(
        long,
        env = "NOTARY_VERSION",
        value_hint = ValueHint::Other,
        default_value = DEFAULT_NOTARY_VERSION
    )]
    pub(crate) notary_version: String,

    /// Notary port (optional for remote notary)
    #[arg(
        long,
        env = "NOTARY_PORT",
        value_hint = ValueHint::Other,
        default_value_t = 443 // Default port for HTTPS
    )]
    pub(crate) notary_port: u16,
}

fn parse_network_setting(s: &str) -> Result<NetworkSetting, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        // primary names
        "latency" => Ok(NetworkSetting::Latency),
        "bandwidth" => Ok(NetworkSetting::Bandwidth),
        // handy aliases
        "throughput" | "tp" => Ok(NetworkSetting::Bandwidth),
        "bw" => Ok(NetworkSetting::Bandwidth),
        "lt" | "low-latency" => Ok(NetworkSetting::Latency),
        other => Err(format!(
            "invalid NETWORK_OPTIMIZATION '{}'; expected one of: latency, bandwidth (aliases: throughput,tp,bw,lt,low-latency)",
            other
        )),
    }
}

fn parse_session_mode(s: &str) -> Result<SessionMode, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "multi" => Ok(SessionMode::Multi),
        "single" => Ok(SessionMode::Single),
        other => Err(format!(
            "invalid SESSION_MODE '{}'; expected one of: multi, single",
            other
        )),
    }
}

fn parse_notary_type(s: &str) -> Result<NotaryMode, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "remote" | "remote_tls" => Ok(NotaryMode::RemoteTLS),
        "remote_non_tls" => Ok(NotaryMode::RemoteNonTLS),
        "ephemeral" => Ok(NotaryMode::Ephemeral),
        other => Err(format!(
            "invalid NOTARY_TYPE '{}'; expected one of: remote, remote_non_tls, ephemeral",
            other
        )),
    }
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    /// Path to the generated proof to verify (optional)
    #[arg(
        value_hint = ValueHint::FilePath,
    )]
    pub(crate) proof_path: Option<PathBuf>,
    /// Flag to by default accepts the key used in the proof
    /// WARNING: this is insecure and should only be used for testing purposes.
    #[arg(
        long,
        env = "ACCEPT_KEY",
        default_value_t = false,
        hide = true // Hide this option from the help output
    )]
    pub(crate) accept_key: bool,
}
