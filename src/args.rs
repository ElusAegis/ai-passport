use clap::ValueHint;
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tlsn_common::config::NetworkSetting;

pub const DEFAULT_NETWORK_OPTIMIZATION: &str = "latency"; // parsed by parser
pub const DEFAULT_SESSION_MODE: &str = "multi-round"; // parsed by parser

pub const DEFAULT_MAX_REQ_NUM_SENT: usize = 3; // e.g., up to 3 model API calls
pub const DEFAULT_MAX_SINGLE_REQUEST_SIZE: usize = 4096; // 4 KiB prompt budget
pub const DEFAULT_MAX_SINGLE_RESPONSE_SIZE: usize = 4096; // 4 KiB response budget

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SessionMode {
    /// Create a fresh protocol instance per request/response pair.
    OneShot,
    /// Keep a single protocol instance across multiple requests (stateless API -> resend history).
    MultiRound,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Prove model interaction
    Prove(ProveArgs),

    /// Verify model interaction
    Verify(VerifyArgs),
}

#[derive(Args, Debug)]
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

    /// Maximum expected number of requests to send
    #[arg(
        long,
        env = "MAX_REQ_NUM_SENT",
        default_value_t = DEFAULT_MAX_REQ_NUM_SENT
    )]
    pub(crate) max_req_num_sent: usize,

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

    /// Session mode (one-shot | multi-round).
    /// one-shot: new protocol per request; exact per-round sizing.
    /// multi-round: single protocol; resend growing history.
    #[arg(
        long,
        env = "SESSION_MODE",
        value_parser = parse_session_mode,
        default_value = DEFAULT_SESSION_MODE
    )]
    pub(crate) session_mode: SessionMode,
}

pub fn parse_network_setting(s: &str) -> Result<NetworkSetting, String> {
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

pub fn parse_session_mode(s: &str) -> Result<SessionMode, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "one-shot" | "oneshot" | "one_shot" => Ok(SessionMode::OneShot),
        "multi-round" | "multiround" | "multi_round" | "multi" => Ok(SessionMode::MultiRound),
        other => Err(format!(
            "invalid SESSION_MODE '{}'; expected one of: one-shot, multi-round",
            other
        )),
    }
}

#[derive(Args, Debug)]
pub(crate) struct VerifyArgs {
    /// Path to the generated proof to verify (optional)
    #[arg(
        long,
        value_hint = ValueHint::FilePath,
        env = "APP_PROOF_PATH"
    )]
    pub(crate) proof_path: Option<String>,
}
