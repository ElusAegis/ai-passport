//! Portfolio snapshot tool.
//!
//! This tool provides the current portfolio state to the agent.
//! Unlike other tools, it doesn't fetch external data but rather
//! formats the current portfolio state for the LLM context.
//!
//! TODO: Fetch portfolio from the agent's on-chain wallet instead of using
//! a local sample portfolio. This would involve:
//! - Connecting to the blockchain RPC endpoint
//! - Reading token balances from the agent's wallet address
//! - Converting on-chain balances to the PortfolioState format

use super::{AttestationMode, Tool, ToolOutput};
use crate::portfolio::PortfolioState;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::Serialize;
use std::time::Instant;

/// Portfolio snapshot tool.
#[derive(Debug, Clone, Default)]
pub struct PortfolioTool;

impl PortfolioTool {
    pub fn new() -> Self {
        Self
    }

    /// Build context JSON from portfolio state.
    fn build_context(&self, portfolio: &PortfolioState) -> Result<String> {
        let positions: Vec<PositionEntry> = portfolio
            .positions()
            .iter()
            .map(|p| PositionEntry {
                symbol: p.symbol.clone(),
                amount: p.amount,
                price_usd: p.price_usd,
                value_usd: p.value_usd(),
                allocation_pct: 0.0, // Will be calculated below
            })
            .collect();

        let total_value: f64 = positions.iter().map(|p| p.value_usd).sum();

        // Calculate allocation percentages
        let positions: Vec<PositionEntry> = positions
            .into_iter()
            .map(|mut p| {
                p.allocation_pct = if total_value > 0.0 {
                    (p.value_usd / total_value * 100.0).round()
                } else {
                    0.0
                };
                p
            })
            .collect();

        let context = PortfolioContext {
            source: "portfolio",
            as_of: Utc::now().to_rfc3339(),
            total_value_usd: total_value.round(),
            position_count: positions.len(),
            positions,
        };

        serde_json::to_string_pretty(&context).context("Failed to serialize portfolio context")
    }
}

#[async_trait]
impl Tool for PortfolioTool {
    fn name(&self) -> &str {
        "Portfolio"
    }

    async fn fetch(
        &self,
        _mode: &AttestationMode,
        portfolio: &PortfolioState,
    ) -> Result<ToolOutput> {
        let start = Instant::now();

        // Portfolio tool doesn't need attestation - it's local state
        let data = self.build_context(portfolio)?;
        let fetch_time_ms = start.elapsed().as_millis() as u64;

        Ok(ToolOutput {
            name: self.name().to_string(),
            data,
            fetch_time_ms,
        })
    }
}

#[derive(Debug, Serialize)]
struct PositionEntry {
    symbol: String,
    amount: f64,
    price_usd: f64,
    value_usd: f64,
    allocation_pct: f64,
}

#[derive(Debug, Serialize)]
struct PortfolioContext {
    source: &'static str,
    as_of: String,
    total_value_usd: f64,
    position_count: usize,
    positions: Vec<PositionEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::Position;

    #[tokio::test]
    async fn test_portfolio_tool() {
        let mut portfolio = PortfolioState::default();
        portfolio.add_position(Position {
            symbol: "BTC".to_string(),
            amount: 1.0,
            price_usd: 50000.0,
        });
        portfolio.add_position(Position {
            symbol: "ETH".to_string(),
            amount: 10.0,
            price_usd: 3000.0,
        });

        let tool = PortfolioTool::new();
        let output = tool
            .fetch(&AttestationMode::Direct, &portfolio)
            .await
            .unwrap();

        assert_eq!(output.name, "Portfolio");
        assert!(output.data.contains("BTC"));
        assert!(output.data.contains("ETH"));
        assert!(output.data.contains("80000")); // total value
        println!("Portfolio context:\n{}", output.data);
    }
}
