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
