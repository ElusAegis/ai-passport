//! Polymarket data fetcher tool.

use super::{AttestationMode, Tool, ToolOutput};
use crate::portfolio::PortfolioState;
use crate::utils::serialization::{de_opt_f64, de_vec_string_flexible};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::time::Instant;

const POLYMARKET_API_DOMAIN: &str = "gamma-api.polymarket.com";

/// Polymarket tool configuration.
#[derive(Debug, Clone)]
pub struct PolymarketTool {
    /// Maximum number of markets to fetch
    pub limit: usize,
    /// Use random pagination (offset 0-4)
    pub random_page: bool,
    /// HTTP client (reused)
    client: Client,
}

impl Default for PolymarketTool {
    fn default() -> Self {
        Self::new(5, false)
    }
}

impl PolymarketTool {
    pub fn new(limit: usize, random_page: bool) -> Self {
        Self {
            limit,
            random_page,
            client: Client::new(),
        }
    }

    /// Fetch markets directly (no attestation).
    async fn fetch_direct(&self) -> Result<Vec<Market>> {
        use rand::Rng;

        let mut url = Url::parse(&format!("https://{}/markets", POLYMARKET_API_DOMAIN))
            .context("Invalid base URL")?;

        // Calculate offset: random page 0-4 if enabled, otherwise 0
        let offset = if self.random_page {
            let page = rand::rng().random_range(0..5);
            tracing::info!("Polymarket: using random page {} (offset {})", page, page * self.limit);
            page * self.limit
        } else {
            0
        };

        url.query_pairs_mut()
            .append_pair("limit", &self.limit.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("tag_id", "21") // Cryptocurrency tag
            .append_pair("related_tags", "true")
            .append_pair("order", "volume")
            .append_pair("ascending", "false")
            .append_pair("active", "true")
            .append_pair("closed", "false");

        let resp = self
            .client
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .context("Failed to send Polymarket request")?
            .error_for_status()
            .context("Polymarket API error")?;

        let markets: Vec<Market> = resp
            .json()
            .await
            .context("Failed to parse Polymarket response")?;

        Ok(markets)
    }

    /// Build compact context JSON from markets.
    fn build_context(&self, markets: &[Market]) -> Result<String> {
        let now = Utc::now();

        let compact_markets: Vec<CompactMarket> = markets
            .iter()
            .map(|m| {
                let outcomes_with_prices: Vec<(String, f64)> = m
                    .outcomes
                    .iter()
                    .zip(m.outcomePrices.iter())
                    .filter_map(|(o, p)| p.parse::<f64>().ok().map(|price| (o.clone(), price)))
                    .collect();

                CompactMarket {
                    id: m.id.clone(),
                    question: m.question.clone().unwrap_or_default(),
                    end_date: m.endDate.clone(),
                    seconds_to_end: m.endDate.as_ref().and_then(|e| {
                        e.parse::<DateTime<Utc>>()
                            .ok()
                            .map(|end| (end - now).num_seconds())
                    }),
                    liquidity: m.liquidity,
                    volume: m.volume,
                    outcomes: outcomes_with_prices,
                }
            })
            .collect();

        let context = PolymarketContext {
            source: "polymarket",
            as_of: now.to_rfc3339(),
            market_count: compact_markets.len(),
            markets: compact_markets,
        };

        serde_json::to_string(&context).context("Failed to serialize Polymarket context")
    }
}

#[async_trait]
impl Tool for PolymarketTool {
    fn name(&self) -> &str {
        "Polymarket"
    }

    async fn fetch(
        &self,
        mode: &AttestationMode,
        _portfolio: &PortfolioState,
    ) -> Result<ToolOutput> {
        let start = Instant::now();

        let markets = match mode {
            AttestationMode::Direct => self.fetch_direct().await?,
            AttestationMode::ProxyTee { .. } => {
                // TODO: Implement proxy-TEE fetch
                anyhow::bail!("ProxyTee mode not yet implemented for Polymarket")
            }
            AttestationMode::TlsNotary { .. } => {
                // TODO: Implement TLSNotary fetch
                anyhow::bail!("TlsNotary mode not yet implemented for Polymarket")
            }
        };

        let data = self.build_context(&markets)?;
        let fetch_time_ms = start.elapsed().as_millis() as u64;

        Ok(ToolOutput {
            name: self.name().to_string(),
            data,
            fetch_time_ms,
        })
    }
}

/// Raw market data from Polymarket API.
#[derive(Debug, Deserialize)]
#[allow(non_snake_case, dead_code)]
struct Market {
    id: String,
    question: Option<String>,
    slug: Option<String>,
    endDate: Option<String>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    liquidity: Option<f64>,
    #[serde(default, deserialize_with = "de_opt_f64")]
    volume: Option<f64>,
    #[serde(default, deserialize_with = "de_vec_string_flexible")]
    outcomes: Vec<String>,
    #[serde(default, deserialize_with = "de_vec_string_flexible")]
    outcomePrices: Vec<String>,
}

/// Compact market for context.
#[derive(Debug, Serialize)]
struct CompactMarket {
    id: String,
    question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seconds_to_end: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    liquidity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<f64>,
    outcomes: Vec<(String, f64)>,
}

/// Polymarket context envelope.
#[derive(Debug, Serialize)]
struct PolymarketContext {
    source: &'static str,
    as_of: String,
    market_count: usize,
    markets: Vec<CompactMarket>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_polymarket_fetch_direct() {
        let tool = PolymarketTool::new(3, false);
        let portfolio = PortfolioState::default();

        let result = tool.fetch(&AttestationMode::Direct, &portfolio).await;

        // This test requires network access
        if let Ok(output) = result {
            assert_eq!(output.name, "Polymarket");
            assert!(!output.data.is_empty());
            println!("Fetch time: {}ms", output.fetch_time_ms);
            println!("Data: {}", output.data);
        }
    }
}
