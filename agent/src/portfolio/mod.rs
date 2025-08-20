use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String, // e.g., "BTC", "ETH"
    pub amount: f64,    // units
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis_usd: Option<f64>, // optional: average cost basis (USD)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Portfolio {
    pub positions: Vec<Position>,
}

impl Portfolio {
    pub fn new(positions: Vec<Position>) -> Self {
        Self { positions }
    }

    /// Fetches the prior context portfolio (placeholder: returns dummy data).
    pub async fn fetch_prior() -> Self {
        // Dummy data: slightly different from current to simulate change
        Self::new(vec![
            Position {
                symbol: "BTC".to_string(),
                amount: 1.2,
                basis_usd: Some(25000.0),
            },
            Position {
                symbol: "ETH".to_string(),
                amount: 8.0,
                basis_usd: Some(1800.0),
            },
            Position {
                symbol: "SOL".to_string(),
                amount: 60.0,
                basis_usd: Some(30.0),
            },
            Position {
                symbol: "USDT".to_string(),
                amount: 5000.0,
                basis_usd: Some(1.0),
            },
            Position {
                symbol: "GOLD".to_string(),
                amount: 2.0,
                basis_usd: Some(1950.0),
            },
        ])
    }

    /// Fetches the current context portfolio (placeholder: returns dummy data).
    pub async fn fetch_current() -> Self {
        // Dummy data: some amounts changed from prior
        Self::new(vec![
            Position {
                symbol: "BTC".to_string(),
                amount: 1.25,
                basis_usd: Some(25000.0),
            },
            Position {
                symbol: "ETH".to_string(),
                amount: 8.5,
                basis_usd: Some(1800.0),
            },
            Position {
                symbol: "SOL".to_string(),
                amount: 58.0,
                basis_usd: Some(30.0),
            },
            Position {
                symbol: "USDT".to_string(),
                amount: 5200.0,
                basis_usd: Some(1.0),
            },
            Position {
                symbol: "GOLD".to_string(),
                amount: 2.0,
                basis_usd: Some(1950.0),
            },
        ])
    }
}
