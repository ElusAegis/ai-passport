use crate::portfolio::price_feed::PriceProvider;
use crate::portfolio::{Portfolio, Position};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricedPosition {
    #[serde(rename = "sym")]
    pub symbol: String,
    #[serde(rename = "amt")]
    pub amount: f64,
    #[serde(rename = "px")]
    pub price_usd: f64,
    #[serde(rename = "val")]
    pub value_usd: f64,
    #[serde(rename = "bs", skip_serializing_if = "Option::is_none")]
    pub basis_usd: Option<f64>,
}

impl PricedPosition {}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PricedPortfolio {
    pub positions: Vec<PricedPosition>,
    #[serde(rename = "tv")]
    pub total_value_usd: f64,
}

pub async fn with_prices<P: PriceProvider>(
    pf: &Portfolio,
    provider: &P,
) -> anyhow::Result<PricedPortfolio> {
    let symbols: Vec<String> = pf.positions.iter().map(|p| p.symbol.clone()).collect();
    let quotes = provider.quotes_usd(&symbols).await?;

    let mut positions = Vec::with_capacity(pf.positions.len());
    let mut total = 0.0_f64;

    for Position {
        symbol,
        amount,
        basis_usd,
    } in &pf.positions
    {
        let px = quotes.get(symbol).map(|q| q.price_usd).unwrap_or(0.0);
        let val = px * *amount;
        total += val;
        positions.push(PricedPosition {
            symbol: symbol.clone(),
            amount: *amount,
            price_usd: px,
            value_usd: val,
            basis_usd: *basis_usd,
        });
    }

    Ok(PricedPortfolio {
        positions,
        total_value_usd: total,
    })
}
