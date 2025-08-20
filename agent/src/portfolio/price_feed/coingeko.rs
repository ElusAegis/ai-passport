use std::collections::{HashMap, HashSet};

use crate::portfolio::price_feed::{PriceProvider, PriceQuote};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::{Client, Url};
use serde::Deserialize;

/// CoinGecko-backed price provider.
/// Fetches spot prices in USD for a set of symbols via `/simple/price`.
pub struct CoingeckoProvider {
    client: Client,
    base: Url,
}

impl Default for CoingeckoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CoingeckoProvider {
    pub fn new() -> Self {
        let client = Client::new();
        let base = Url::parse("https://api.coingecko.com/api/v3/simple/price").expect("valid url");
        Self { client, base }
    }

    /// Map a ticker symbol to CoinGecko's asset id.
    pub fn map_symbol_to_id(sym: &str) -> Option<&'static str> {
        match sym.to_ascii_uppercase().as_str() {
            // Majors
            "BTC" => Some("bitcoin"),
            "ETH" => Some("ethereum"),
            "SOL" => Some("solana"),
            // Stables
            "USDT" => Some("tether"),
            "USDC" => Some("usd-coin"),
            "DAI" => Some("dai"),
            // Gold proxies
            "PAXG" | "GOLD" | "XAU" => Some("pax-gold"),
            _ => None,
        }
    }
}

#[async_trait]
impl PriceProvider for CoingeckoProvider {
    async fn quotes_usd(&self, symbols: &[String]) -> Result<HashMap<String, PriceQuote>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        // Deduplicate and map to CoinGecko ids
        let mut ids: Vec<&'static str> = Vec::new();
        let mut sym_to_id: HashMap<String, &'static str> = HashMap::new();
        let mut seen = HashSet::new();
        for s in symbols {
            let id =
                Self::map_symbol_to_id(s).ok_or_else(|| anyhow!("unsupported symbol: {}", s))?;
            if seen.insert(id) {
                ids.push(id);
            }
            sym_to_id.insert(s.clone(), id);
        }

        // Build URL with query params
        let mut url = self.base.clone();
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("ids", &ids.join(","));
            qp.append_pair("vs_currencies", "usd");
        }

        // Send request
        let resp = self
            .client
            .get(url)
            .header("accept", "application/json")
            .send()
            .await
            .context("coingecko: request failed")?
            .error_for_status()
            .context("coingecko: non-success status")?;

        let body = resp.bytes().await.context("coingecko: read body failed")?;

        // Parse like: { "bitcoin": {"usd": 12345.6}, ... }
        let parsed: HashMap<String, HashMap<String, f64>> =
            serde_json::from_slice(&body).context("coingecko: parse JSON failed")?;

        // Validate all requested ids are present and have usd
        let now = Utc::now().timestamp() as u64;
        let mut out: HashMap<String, PriceQuote> = HashMap::new();
        for (sym, id) in sym_to_id.iter() {
            let rec = parsed
                .get(*id)
                .ok_or_else(|| anyhow!("coingecko: id missing in response: {}", id))?;
            let px = rec
                .get("usd")
                .ok_or_else(|| anyhow!("coingecko: usd missing for id: {}", id))?;
            out.insert(
                sym.clone(),
                PriceQuote {
                    symbol: sym.clone(),
                    price_usd: *px,
                    as_of_unix: now,
                },
            );
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::CoingeckoProvider;
    use std::assert_matches::assert_matches;

    #[test]
    fn mapping_known_symbols() {
        assert_matches!(CoingeckoProvider::map_symbol_to_id("btc"), Some("bitcoin"));
        assert_matches!(CoingeckoProvider::map_symbol_to_id("ETH"), Some("ethereum"));
        assert_matches!(CoingeckoProvider::map_symbol_to_id("SOL"), Some("solana"));
        assert_matches!(CoingeckoProvider::map_symbol_to_id("USDT"), Some("tether"));
        assert_matches!(
            CoingeckoProvider::map_symbol_to_id("GOLD"),
            Some("pax-gold")
        );
        assert_matches!(CoingeckoProvider::map_symbol_to_id("XAU"), Some("pax-gold"));
        assert!(CoingeckoProvider::map_symbol_to_id("NOPE").is_none());
    }

    #[test]
    fn mapping_unsupported_symbol_returns_none() {
        assert!(CoingeckoProvider::map_symbol_to_id("DOGE").is_none());
        assert!(CoingeckoProvider::map_symbol_to_id("random").is_none());
    }

    // A fake handler to test error for missing id or usd key
    #[test]
    fn handle_missing_id_or_usd() {
        let parsed: std::collections::HashMap<String, std::collections::HashMap<String, f64>> = [
            (
                "bitcoin".to_string(),
                [("usd".to_string(), 12345.0)].iter().cloned().collect(),
            ),
            // "ethereum" missing
        ]
        .iter()
        .cloned()
        .collect();
        // Simulate the logic for missing id
        let id = "ethereum";
        let result = parsed
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("coingecko: id missing in response: {}", id));
        assert!(result.is_err());

        // Simulate missing "usd" key
        let parsed2: std::collections::HashMap<String, std::collections::HashMap<String, f64>> =
            [(
                "bitcoin".to_string(),
                [("eur".to_string(), 10000.0)].iter().cloned().collect(),
            )]
            .iter()
            .cloned()
            .collect();
        let rec = parsed2.get("bitcoin").unwrap();
        let result2 = rec
            .get("usd")
            .ok_or_else(|| anyhow::anyhow!("coingecko: usd missing for id: bitcoin"));
        assert!(result2.is_err());
    }
}
