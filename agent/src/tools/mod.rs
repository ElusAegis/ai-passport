//! Tool abstractions for the trading agent.
//!
//! Tools are data fetchers that provide context to the agent.
//! Each tool can operate in different attestation modes.

pub mod coingecko;
pub mod polymarket;
pub mod portfolio;

use crate::portfolio::PortfolioState;
use anyhow::Result;
use async_trait::async_trait;

/// Attestation mode for tool data fetching.
#[derive(Debug, Clone, Default)]
pub enum ToolAttestationMode {
    /// Direct API calls, no attestation
    #[default]
    Direct,
    /// Route through TEE proxy for attestation
    Proxy { host: String, port: u16 },
}

/// Output from a tool fetch operation.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Name of the tool
    pub name: String,
    /// JSON data from the tool
    pub data: String,
    /// Time taken to fetch (milliseconds)
    pub fetch_time_ms: u64,
}

/// Trait for tools that provide data to the agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name.
    fn name(&self) -> &str;

    /// Fetch data from the tool.
    ///
    /// The `mode` parameter determines how the fetch is performed
    /// (direct or via proxy-TEE).
    ///
    /// The `portfolio` parameter provides access to current holdings
    /// for tools that need it (e.g., price feed needs to know which assets).
    async fn fetch(
        &self,
        mode: &ToolAttestationMode,
        portfolio: &PortfolioState,
    ) -> Result<ToolOutput>;
}
