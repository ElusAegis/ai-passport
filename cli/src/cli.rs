use crate::config::notary::NotaryMode;
use crate::prover::{ProverKind, ProxyConfig};
use crate::NotaryConfig;
use clap::ValueHint;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use tlsn_common::config::NetworkSetting;

pub const DEFAULT_PROVER: &str = "tls-single";

// Byte budget defaults for TLS notarization
pub const DEFAULT_MAX_SENT_BYTES: usize = 4 * 1024; // 4 KiB total budget
pub const DEFAULT_MAX_RECV_BYTES: usize = 16 * 1024; // 16 KiB total budget

// Notary defaults
pub const DEFAULT_NOTARY_TYPE: &str = "remote";
pub const DEFAULT_NOTARY_DOMAIN: &str = "notary.pse.dev";
pub const DEFAULT_NOTARY_VERSION: &str = "v0.1.0-alpha.12";
pub const DEFAULT_NOTARY_PORT: u16 = 443;
pub const DEFAULT_NETWORK_OPTIMIZATION: &str = "latency";

// Proxy defaults
pub const DEFAULT_PROXY_HOST: &str = "localhost";
pub const DEFAULT_PROXY_PORT: u16 = 8443;

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

/// Notary server configuration (only used with tls-single and tls-per-message provers)
#[derive(Args, Clone, Debug)]
pub struct NotaryArgs {
    /// Notary type (remote | ephemeral)
    /// remote (remote_tls): use a remote notary server with TLS.
    /// remote_non_tls: use a remote notary server without TLS.
    /// ephemeral: use an ephemeral notary server that spins up locally.
    #[arg(
        long = "notary-type",
        env = "NOTARY_TYPE",
        value_parser = parse_notary_type,
        default_value = DEFAULT_NOTARY_TYPE,
    )]
    pub mode: NotaryMode,

    /// Notary server domain
    #[arg(
        id = "notary_domain",
        long = "notary-domain",
        env = "NOTARY_DOMAIN",
        value_hint = ValueHint::Hostname,
        default_value = DEFAULT_NOTARY_DOMAIN,
    )]
    pub domain: String,

    /// Notary API version prefix
    #[arg(
        long = "notary-version",
        env = "NOTARY_VERSION",
        value_hint = ValueHint::Other,
        default_value = DEFAULT_NOTARY_VERSION
    )]
    pub version: String,

    /// Notary server port
    #[arg(
        id = "notary_port",
        long = "notary-port",
        env = "NOTARY_PORT",
        value_hint = ValueHint::Other,
        default_value_t = DEFAULT_NOTARY_PORT
    )]
    pub port: u16,

    /// Network optimization strategy (latency | bandwidth)
    #[arg(
        long = "notary-network-optimization",
        env = "NOTARY_NETWORK_OPTIMIZATION",
        value_parser = parse_network_setting,
        default_value = DEFAULT_NETWORK_OPTIMIZATION
    )]
    pub network_optimization: NetworkSetting,

    /// Maximum bytes to send over the TLS session
    #[arg(
        long = "notary-max-sent-bytes",
        env = "NOTARY_MAX_SENT_BYTES",
        default_value_t = DEFAULT_MAX_SENT_BYTES
    )]
    pub max_sent_bytes: usize,

    /// Maximum bytes to receive over the TLS session
    #[arg(
        long = "notary-max-recv-bytes",
        env = "NOTARY_MAX_RECV_BYTES",
        default_value_t = DEFAULT_MAX_RECV_BYTES
    )]
    pub max_recv_bytes: usize,
}

impl TryFrom<NotaryArgs> for NotaryConfig {
    type Error = anyhow::Error;

    fn try_from(args: NotaryArgs) -> anyhow::Result<Self> {
        NotaryConfig::builder()
            .domain(args.domain)
            .mode(args.mode)
            .path_prefix(args.version)
            .port(args.port)
            .network_optimization(args.network_optimization)
            .defer_decryption(false)
            .max_decrypted_online(args.max_recv_bytes)
            .max_total_recv(args.max_recv_bytes)
            .max_total_sent(args.max_sent_bytes)
            .build()
            .map_err(Into::into)
    }
}

/// Proxy server configuration (only used with proxy prover)
#[derive(Args, Clone, Debug)]
pub struct ProxyArgs {
    /// Proxy server host
    #[arg(
        id = "proxy_host",
        long = "proxy-host",
        env = "PROXY_HOST",
        value_hint = ValueHint::Hostname,
        default_value = DEFAULT_PROXY_HOST
    )]
    pub host: String,

    /// Proxy server port
    #[arg(
        id = "proxy_port",
        long = "proxy-port",
        env = "PROXY_PORT",
        value_hint = ValueHint::Other,
        default_value_t = DEFAULT_PROXY_PORT
    )]
    pub port: u16,
}

impl From<ProxyArgs> for ProxyConfig {
    fn from(args: ProxyArgs) -> Self {
        ProxyConfig {
            host: args.host,
            port: args.port,
        }
    }
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

    /// Prover type to use for generating proofs.
    /// - direct: passthrough without proving (for testing)
    /// - proxy: connect through attestation proxy server
    /// - tls-single: single TLS session, one proof at end
    /// - tls-per-message: fresh TLS per message, proof per message
    #[arg(
        long,
        env = "PROVER",
        value_parser = parse_prover_kind,
        default_value = DEFAULT_PROVER
    )]
    pub(crate) prover: ProverKind,

    /// Proxy configuration (only used with proxy prover)
    #[command(flatten)]
    pub(crate) proxy: ProxyArgs,

    /// Notary configuration (only used with TLS provers)
    #[command(flatten)]
    pub(crate) notary: NotaryArgs,
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

fn parse_prover_kind(s: &str) -> Result<ProverKind, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        // New names
        "direct" | "passthrough" | "none" => Ok(ProverKind::Direct),
        "proxy" => Ok(ProverKind::Proxy),
        "tls-single" | "tls-single-shot" => Ok(ProverKind::TlsSingleShot),
        "tls-per-message" | "tls-multi" => Ok(ProverKind::TlsPerMessage),
        // Backwards-compatible aliases for old --session-mode values
        "single" => Ok(ProverKind::TlsSingleShot),
        "multi" => Ok(ProverKind::TlsPerMessage),
        other => Err(format!(
            "invalid PROVER '{}'; expected one of: direct, proxy, tls-single, tls-per-message (aliases: single, multi)",
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
