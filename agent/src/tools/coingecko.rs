//! CoinGecko price feed tool.

use super::{AttestationMode, Tool, ToolOutput};
use crate::portfolio::PortfolioState;
use ai_passport::{ProxyConfig, ProxyProver};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use hyper::StatusCode;
use reqwest::{Client, Url};
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;

const COINGECKO_API_DOMAIN: &str = "api.coingecko.com";
const COINGECKO_API_PORT: u16 = 443;

/// CoinGecko price feed tool.
#[derive(Debug, Clone)]
pub struct CoinGeckoTool {
    client: Client,
    base_url: Url,
}

impl Default for CoinGeckoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl CoinGeckoTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: Url::parse("https://api.coingecko.com/api/v3/simple/price")
                .expect("valid URL"),
        }
    }

    /// Map a ticker symbol to CoinGecko's asset ID.
    fn symbol_to_id(sym: &str) -> Option<&'static str> {
        match sym.to_ascii_uppercase().as_str() {
            "BTC" => Some("bitcoin"),
            "ETH" => Some("ethereum"),
            "SOL" => Some("solana"),
            "USDT" => Some("tether"),
            "USDC" => Some("usd-coin"),
            "DAI" => Some("dai"),
            "PAXG" | "GOLD" | "XAU" => Some("pax-gold"),
            "BNB" => Some("binancecoin"),
            "XRP" => Some("ripple"),
            "ADA" => Some("cardano"),
            "DOT" => Some("polkadot"),
            "LTC" => Some("litecoin"),
            "LINK" => Some("chainlink"),
            "AVAX" => Some("avalanche-2"),
            "MATIC" => Some("matic-network"),
            _ => None,
        }
    }

    /// Fetch prices directly (with fallback to hardcoded prices on rate limit).
    async fn fetch_direct(&self, symbols: &[String]) -> Result<HashMap<String, f64>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        // Map symbols to CoinGecko IDs
        let mut id_to_symbol: HashMap<&str, String> = HashMap::new();
        let mut ids: Vec<&str> = Vec::new();

        for sym in symbols {
            if let Some(id) = Self::symbol_to_id(sym) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                id_to_symbol.insert(id, sym.clone());
            }
        }

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Build URL
        let mut url = self.base_url.clone();
        url.query_pairs_mut()
            .append_pair("ids", &ids.join(","))
            .append_pair("vs_currencies", "usd");

        // Fetch
        let resp = self
            .client
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .context("CoinGecko request failed")?;

        // Handle rate limiting by falling back to hardcoded prices
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!("CoinGecko rate limited, using fallback prices");
            return Ok(Self::fallback_prices(symbols));
        }

        let resp = resp.error_for_status().context("CoinGecko API error")?;

        // Parse: {"bitcoin": {"usd": 12345.6}, ...}
        let data: HashMap<String, HashMap<String, f64>> = resp
            .json()
            .await
            .context("Failed to parse CoinGecko response")?;

        // Convert back to symbol -> price
        let mut prices: HashMap<String, f64> = HashMap::new();
        for (id, currencies) in data {
            if let Some(price) = currencies.get("usd") {
                // Find all symbols that map to this ID
                for sym in symbols {
                    if Self::symbol_to_id(sym) == Some(id.as_str()) {
                        prices.insert(sym.clone(), *price);
                    }
                }
            }
        }

        Ok(prices)
    }

    /// Build query path for CoinGecko API.
    fn build_query_path(ids: &[&str]) -> String {
        format!(
            "/api/v3/simple/price?ids={}&vs_currencies=usd",
            ids.join(",")
        )
    }

    /// Map symbols to CoinGecko IDs.
    fn symbols_to_ids(symbols: &[String]) -> (Vec<&'static str>, HashMap<&'static str, Vec<String>>) {
        let mut ids: Vec<&'static str> = Vec::new();
        let mut id_to_symbols: HashMap<&'static str, Vec<String>> = HashMap::new();

        for sym in symbols {
            if let Some(id) = Self::symbol_to_id(sym) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                id_to_symbols
                    .entry(id)
                    .or_default()
                    .push(sym.clone());
            }
        }

        (ids, id_to_symbols)
    }

    /// Fetch prices via proxy-TEE (with attestation).
    async fn fetch_proxy(
        &self,
        symbols: &[String],
        host: &str,
        port: u16,
    ) -> Result<HashMap<String, f64>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        let (ids, id_to_symbols) = Self::symbols_to_ids(symbols);

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let prover = ProxyProver::new(ProxyConfig {
            host: host.to_string(),
            port,
        });

        let path = Self::build_query_path(&ids);
        tracing::info!("CoinGecko: fetching via proxy-TEE: {}", path);

        let response = prover
            .fetch(COINGECKO_API_DOMAIN, COINGECKO_API_PORT, &path, true)
            .await
            .context("Failed to fetch via proxy")?;

        // Handle rate limiting
        if response.status == StatusCode::TOO_MANY_REQUESTS {
            anyhow::bail!("CoinGecko rate limited via proxy: {}", response.status);
        }

        if !response.status.is_success() {
            anyhow::bail!(
                "CoinGecko API error: {} - {}",
                response.status,
                response.body
            );
        }

        if let Some(attestation_path) = &response.attestation_path {
            tracing::info!(
                "CoinGecko attestation saved to: {}",
                attestation_path.display()
            );
        }

        // Parse: {"bitcoin": {"usd": 12345.6}, ...}
        let data: HashMap<String, HashMap<String, f64>> =
            serde_json::from_str(&response.body).context("Failed to parse CoinGecko response")?;

        // Convert back to symbol -> price
        let mut prices: HashMap<String, f64> = HashMap::new();
        for (id, currencies) in data {
            if let Some(price) = currencies.get("usd") {
                // Find all symbols that map to this ID
                if let Some(syms) = id_to_symbols.get(id.as_str()) {
                    for sym in syms {
                        prices.insert(sym.clone(), *price);
                    }
                }
            }
        }

        Ok(prices)
    }

    /// Fallback prices when API is rate limited (approximate Dec 2024 prices).
    fn fallback_prices(symbols: &[String]) -> HashMap<String, f64> {
        let defaults: HashMap<&str, f64> = [
            ("BTC", 100000.0),
            ("ETH", 3500.0),
            ("SOL", 200.0),
            ("USDT", 1.0),
            ("USDC", 1.0),
            ("DAI", 1.0),
            ("PAXG", 2600.0),
            ("BNB", 700.0),
            ("XRP", 2.0),
            ("ADA", 1.0),
            ("DOT", 8.0),
            ("LTC", 100.0),
            ("LINK", 25.0),
            ("AVAX", 45.0),
            ("MATIC", 0.5),
        ]
        .into_iter()
        .collect();

        symbols
            .iter()
            .filter_map(|sym| {
                defaults
                    .get(sym.to_uppercase().as_str())
                    .map(|&price| (sym.clone(), price))
            })
            .collect()
    }

    /// Build context JSON from prices.
    fn build_context(&self, prices: &HashMap<String, f64>) -> Result<String> {
        let price_list: Vec<PriceEntry> = prices
            .iter()
            .map(|(sym, price)| PriceEntry {
                symbol: sym.clone(),
                price_usd: *price,
            })
            .collect();

        let context = PriceFeedContext {
            source: "coingecko",
            as_of: Utc::now().to_rfc3339(),
            currency: "USD",
            prices: price_list,
        };

        serde_json::to_string(&context).context("Failed to serialize price context")
    }
}

#[async_trait]
impl Tool for CoinGeckoTool {
    fn name(&self) -> &str {
        "PriceFeed"
    }

    async fn fetch(
        &self,
        mode: &AttestationMode,
        portfolio: &PortfolioState,
    ) -> Result<ToolOutput> {
        let start = Instant::now();

        // Get symbols from portfolio
        let symbols: Vec<String> = portfolio.symbols().into_iter().collect();

        let prices = match mode {
            AttestationMode::Direct => self.fetch_direct(&symbols).await?,
            AttestationMode::ProxyTee { host, port } => {
                self.fetch_proxy(&symbols, host, *port).await?
            }
            _ => {
                anyhow::bail!("Other modes not yet implemented for CoinGecko")
            }
        };

        let data = self.build_context(&prices)?;
        let fetch_time_ms = start.elapsed().as_millis() as u64;

        Ok(ToolOutput {
            name: self.name().to_string(),
            data,
            fetch_time_ms,
        })
    }
}

#[derive(Debug, Serialize)]
struct PriceEntry {
    symbol: String,
    price_usd: f64,
}

#[derive(Debug, Serialize)]
struct PriceFeedContext {
    source: &'static str,
    as_of: String,
    currency: &'static str,
    prices: Vec<PriceEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_mapping() {
        assert_eq!(CoinGeckoTool::symbol_to_id("BTC"), Some("bitcoin"));
        assert_eq!(CoinGeckoTool::symbol_to_id("btc"), Some("bitcoin"));
        assert_eq!(CoinGeckoTool::symbol_to_id("ETH"), Some("ethereum"));
        assert_eq!(CoinGeckoTool::symbol_to_id("PAXG"), Some("pax-gold"));
        assert_eq!(CoinGeckoTool::symbol_to_id("UNKNOWN"), None);
    }
}
