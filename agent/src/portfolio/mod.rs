use serde::{Deserialize, Serialize};

pub(crate) mod fetch;
pub(crate) mod price_feed;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    #[serde(rename = "sym")]
    pub symbol: String, // BTC, ETH, SOL, USDT, GOLD
    #[serde(rename = "amt")]
    pub amount: f64, // units
    #[serde(rename = "bs", skip_serializing_if = "Option::is_none")]
    pub basis_usd: Option<f64>, // avg cost (USD) if known
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Portfolio {
    pub positions: Vec<Position>,
}

impl Portfolio {
    pub fn new(positions: Vec<Position>) -> Self {
        Self { positions }
    }
}
