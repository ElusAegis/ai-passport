//! Mutable portfolio state management.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// A position in the portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub amount: f64,
    pub price_usd: f64,
}

impl Position {
    /// Calculate the USD value of this position.
    pub fn value_usd(&self) -> f64 {
        self.amount * self.price_usd
    }
}

/// Manages the current portfolio state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioState {
    positions: Vec<Position>,
}

impl PortfolioState {
    /// Create an empty portfolio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a portfolio from a list of positions.
    pub fn from_positions(positions: Vec<Position>) -> Self {
        Self { positions }
    }

    /// Add a position to the portfolio.
    pub fn add_position(&mut self, position: Position) {
        // Check if we already have this symbol
        if let Some(existing) = self
            .positions
            .iter_mut()
            .find(|p| p.symbol == position.symbol)
        {
            existing.amount += position.amount;
            existing.price_usd = position.price_usd; // Update price
        } else {
            self.positions.push(position);
        }
    }

    /// Get all positions.
    pub fn positions(&self) -> &[Position] {
        &self.positions
    }

    /// Get a position by symbol.
    pub fn get(&self, symbol: &str) -> Option<&Position> {
        self.positions.iter().find(|p| p.symbol == symbol)
    }

    /// Get a mutable position by symbol.
    pub fn get_mut(&mut self, symbol: &str) -> Option<&mut Position> {
        self.positions.iter_mut().find(|p| p.symbol == symbol)
    }

    /// Get all symbols in the portfolio.
    pub fn symbols(&self) -> HashSet<String> {
        self.positions.iter().map(|p| p.symbol.clone()).collect()
    }

    /// Calculate total portfolio value in USD.
    pub fn total_value_usd(&self) -> f64 {
        self.positions.iter().map(|p| p.value_usd()).sum()
    }

    /// Update prices for all positions.
    pub fn update_prices(&mut self, prices: &std::collections::HashMap<String, f64>) {
        for position in &mut self.positions {
            if let Some(&price) = prices.get(&position.symbol) {
                position.price_usd = price;
            }
        }
    }

    /// Execute a swap: sell `amount_usd` of `from` asset and buy `to` asset.
    pub fn execute_swap(&mut self, from: &str, to: &str, amount_usd: f64) -> Result<()> {
        // Get the 'from' position
        let from_pos = self
            .get(from)
            .with_context(|| format!("Asset '{}' not in portfolio", from))?;

        let from_value = from_pos.value_usd();
        if amount_usd > from_value {
            anyhow::bail!(
                "Insufficient balance: trying to swap ${} but only have ${:.2} in {}",
                amount_usd,
                from_value,
                from
            );
        }

        let from_price = from_pos.price_usd;
        let from_amount_to_sell = amount_usd / from_price;

        // Get the 'to' position (or create it)
        let to_price = self.get(to).map(|p| p.price_usd).unwrap_or(1.0);

        // Reduce 'from' position
        if let Some(pos) = self.get_mut(from) {
            pos.amount -= from_amount_to_sell;
            // Remove if amount is negligible
            if pos.amount < 0.0001 {
                pos.amount = 0.0;
            }
        }

        // Increase 'to' position
        let to_amount_to_buy = amount_usd / to_price;
        if let Some(pos) = self.get_mut(to) {
            pos.amount += to_amount_to_buy;
        } else {
            // Create new position
            self.positions.push(Position {
                symbol: to.to_string(),
                amount: to_amount_to_buy,
                price_usd: to_price,
            });
        }

        // Remove positions with zero amount
        self.positions.retain(|p| p.amount > 0.0001);

        Ok(())
    }

    /// Create a sample portfolio for testing.
    pub fn sample() -> Self {
        Self::from_positions(vec![
            Position {
                symbol: "BTC".to_string(),
                amount: 0.5,
                price_usd: 100000.0,
            },
            Position {
                symbol: "ETH".to_string(),
                amount: 5.0,
                price_usd: 3500.0,
            },
            Position {
                symbol: "SOL".to_string(),
                amount: 50.0,
                price_usd: 200.0,
            },
            Position {
                symbol: "USDT".to_string(),
                amount: 10000.0,
                price_usd: 1.0,
            },
            Position {
                symbol: "PAXG".to_string(),
                amount: 2.0,
                price_usd: 2600.0,
            },
        ])
    }

    /// Format portfolio state as a detailed string for logging.
    pub fn format_detailed(&self) -> String {
        let mut lines = Vec::new();
        lines.push("┌─────────────────────────────────────────────────────┐".to_string());
        lines.push("│                  PORTFOLIO STATE                    │".to_string());
        lines.push("├──────────┬───────────────┬──────────────┬───────────┤".to_string());
        lines.push("│  Symbol  │     Amount    │    Price     │   Value   │".to_string());
        lines.push("├──────────┼───────────────┼──────────────┼───────────┤".to_string());

        // Sort positions by value (descending)
        let mut sorted_positions: Vec<_> = self.positions.iter().collect();
        sorted_positions.sort_by(|a, b| b.value_usd().partial_cmp(&a.value_usd()).unwrap());

        for pos in sorted_positions {
            let value = pos.value_usd();
            lines.push(format!(
                "│ {:>8} │ {:>13.6} │ ${:>10.2} │ ${:>8.2} │",
                pos.symbol, pos.amount, pos.price_usd, value
            ));
        }

        lines.push("├──────────┴───────────────┴──────────────┼───────────┤".to_string());
        lines.push(format!(
            "│                              TOTAL      │ ${:>8.2} │",
            self.total_value_usd()
        ));
        lines.push("└──────────────────────────────────────────┴───────────┘".to_string());

        lines.join("\n")
    }
}

impl fmt::Display for PortfolioState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_detailed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portfolio_value() {
        let portfolio = PortfolioState::from_positions(vec![
            Position {
                symbol: "BTC".to_string(),
                amount: 1.0,
                price_usd: 50000.0,
            },
            Position {
                symbol: "ETH".to_string(),
                amount: 10.0,
                price_usd: 3000.0,
            },
        ]);

        assert_eq!(portfolio.total_value_usd(), 80000.0);
    }

    #[test]
    fn test_swap_execution() {
        let mut portfolio = PortfolioState::from_positions(vec![
            Position {
                symbol: "USDT".to_string(),
                amount: 10000.0,
                price_usd: 1.0,
            },
            Position {
                symbol: "BTC".to_string(),
                amount: 0.1,
                price_usd: 50000.0,
            },
        ]);

        // Swap $1000 USDT -> BTC
        portfolio.execute_swap("USDT", "BTC", 1000.0).unwrap();

        assert_eq!(portfolio.get("USDT").unwrap().amount, 9000.0);
        assert!((portfolio.get("BTC").unwrap().amount - 0.12).abs() < 0.001);
    }

    #[test]
    fn test_swap_insufficient_balance() {
        let mut portfolio = PortfolioState::from_positions(vec![Position {
            symbol: "USDT".to_string(),
            amount: 100.0,
            price_usd: 1.0,
        }]);

        let result = portfolio.execute_swap("USDT", "BTC", 1000.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_swap_creates_new_position() {
        let mut portfolio = PortfolioState::from_positions(vec![Position {
            symbol: "USDT".to_string(),
            amount: 10000.0,
            price_usd: 1.0,
        }]);

        assert!(portfolio.get("BTC").is_none());

        portfolio.execute_swap("USDT", "BTC", 1000.0).unwrap();

        assert!(portfolio.get("BTC").is_some());
    }
}
