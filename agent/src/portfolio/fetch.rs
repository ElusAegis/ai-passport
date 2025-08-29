use crate::portfolio::{Portfolio, Position};

/// Placeholder: fetch the **current** portfolio (later: DB/API).
pub async fn fetch_current() -> Portfolio {
    Portfolio::new(vec![
        Position {
            symbol: "BTC".into(),
            amount: 1.25,
            basis_usd: Some(25_000.0),
        },
        Position {
            symbol: "ETH".into(),
            amount: 8.5,
            basis_usd: Some(1_800.0),
        },
        Position {
            symbol: "SOL".into(),
            amount: 58.0,
            basis_usd: Some(30.0),
        },
        Position {
            symbol: "USDT".into(),
            amount: 5200.0,
            basis_usd: Some(1.0),
        },
        Position {
            symbol: "PAXG".into(),
            amount: 2.0,
            basis_usd: Some(1_950.0),
        },
    ])
}
