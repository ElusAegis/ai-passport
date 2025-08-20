pub(crate) mod coingeko;
pub(crate) mod context;
pub(crate) mod enrich;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceQuote {
    pub symbol: String,  // "BTC", "ETH", ...
    pub price_usd: f64,  // last price USD
    pub as_of_unix: u64, // seconds
}

#[async_trait]
pub trait PriceProvider: Send + Sync {
    async fn quotes_usd(&self, symbols: &[String]) -> anyhow::Result<HashMap<String, PriceQuote>>;
}

/// Dummy provider for now; replace with Coingecko / exchange later.
pub struct DummyPriceProvider;

#[async_trait]
impl PriceProvider for DummyPriceProvider {
    async fn quotes_usd(&self, symbols: &[String]) -> anyhow::Result<HashMap<String, PriceQuote>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut m = HashMap::new();
        for s in symbols {
            let price = match s.as_str() {
                "BTC" => 62_000.0,
                "ETH" => 2_900.0,
                "SOL" => 145.0,
                "USDT" => 1.0,
                "PAXG" | "XAU" => 2_350.0,
                _ => 0.0,
            };
            m.insert(
                s.clone(),
                PriceQuote {
                    symbol: s.clone(),
                    price_usd: price,
                    as_of_unix: now,
                },
            );
        }
        Ok(m)
    }
}
