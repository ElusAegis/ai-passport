//! CLI argument parsing for the VeriTrade agent.
//!
//! Uses clap for argument parsing with environment variable fallbacks.

use ai_passport::ProverKind;
use clap::{Parser, ValueHint};

/// Default prover type - direct mode for testing without proofs
pub const DEFAULT_PROVER: &str = "direct";

/// Tool attestation kind (simpler than ProverKind - only direct or proxy)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ToolAttestationKind {
    /// Direct API calls, no attestation
    #[default]
    Direct,
    /// Route through TEE proxy for attestation
    Proxy,
}

/// VeriTrade - Autonomous AI Trading Agent
///
/// An AI-powered portfolio manager that analyzes market data and makes
/// trading decisions. Supports multiple attestation modes for verifiable
/// execution.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct AgentArgs {
    /// API domain for the LLM provider (e.g., api.anthropic.com)
    #[arg(
        long = "api-domain",
        env = "MODEL_API_DOMAIN",
        value_hint = ValueHint::Hostname
    )]
    pub api_domain: String,

    /// API port (default: 443)
    #[arg(long = "api-port", env = "MODEL_API_PORT", default_value = "443")]
    pub api_port: u16,

    /// Number of trading rounds to execute
    #[arg(
        short = 'r',
        long = "rounds",
        env = "AGENT_ROUNDS",
        default_value = "1"
    )]
    pub rounds: usize,

    /// Delay between rounds in seconds (0 = no delay)
    #[arg(
        short = 'd',
        long = "round-delay",
        env = "AGENT_ROUND_DELAY",
        default_value = "0"
    )]
    pub round_delay: u64,

    /// Number of Polymarket markets to fetch
    #[arg(
        short = 'p',
        long = "polymarket-limit",
        env = "POLYMARKET_LIMIT",
        default_value = "5"
    )]
    pub polymarket_limit: usize,

    /// Use random pagination for Polymarket (offset 0-4)
    #[arg(long = "polymarket-random-page", default_value = "false")]
    pub polymarket_random_page: bool,

    /// Specify the model to use
    #[arg(long = "model", env = "MODEL_ID")]
    pub model_id: Option<String>,

    /// Prover type to use for LLM interactions.
    /// - direct: passthrough without proving (for testing)
    /// - proxy: connect through attestation proxy server
    /// - tls-single: single TLS session, one proof at end
    /// - tls-per-message: fresh TLS per message, proof per message
    #[arg(
        long = "prover",
        env = "PROVER",
        value_parser = parse_prover_kind,
        default_value = DEFAULT_PROVER
    )]
    pub prover: ProverKind,

    /// Attestation mode for tool data fetching (Polymarket, CoinGecko).
    /// - direct: fetch directly without attestation
    /// - proxy: route through TEE proxy for attestation
    #[arg(
        long = "tool-attestation",
        env = "TOOL_ATTESTATION",
        value_parser = parse_tool_attestation_kind,
        default_value = "direct"
    )]
    pub tool_attestation: ToolAttestationKind,
}

/// Parse prover kind from string (mirrors CLI crate's parser).
fn parse_prover_kind(s: &str) -> Result<ProverKind, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "direct" | "passthrough" | "none" => Ok(ProverKind::Direct),
        "proxy" => Ok(ProverKind::Proxy),
        "tls-single" | "tls-single-shot" => Ok(ProverKind::TlsSingleShot),
        "tls-per-message" | "tls-multi" => Ok(ProverKind::TlsPerMessage),
        "single" => Ok(ProverKind::TlsSingleShot),
        "multi" => Ok(ProverKind::TlsPerMessage),
        other => Err(format!(
            "invalid PROVER '{}'; expected one of: direct, proxy, tls-single, tls-per-message",
            other
        )),
    }
}

/// Parse tool attestation kind from string.
fn parse_tool_attestation_kind(s: &str) -> Result<ToolAttestationKind, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "direct" | "none" => Ok(ToolAttestationKind::Direct),
        "proxy" | "proxy-tee" | "tee" => Ok(ToolAttestationKind::Proxy),
        other => Err(format!(
            "invalid TOOL_ATTESTATION '{}'; expected one of: direct, proxy",
            other
        )),
    }
}
