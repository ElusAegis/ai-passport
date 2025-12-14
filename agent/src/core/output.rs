//! Structured output parsing for agent decisions.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// A trading decision from the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeDecision {
    /// Brief market summary
    pub summary: String,
    /// Market observations/signals
    #[serde(default)]
    pub observations: Vec<Observation>,
    /// Proposed trades
    #[serde(default)]
    pub trades: Vec<Trade>,
}

/// A market observation/signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Description of the signal
    pub signal: String,
    /// Confidence level
    pub confidence: Confidence,
}

/// Confidence level for an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// A single trade instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    /// Asset to sell
    pub from: String,
    /// Asset to buy
    pub to: String,
    /// Amount in USD to swap
    pub amount_usd: f64,
    /// Reason for the trade
    pub reason: String,
}

impl TradeDecision {
    /// Parse a trade decision from an LLM response string.
    pub fn parse(response: &str) -> Result<Self> {
        // Try to extract JSON from the response
        let json_str = extract_json(response)?;

        // Parse the JSON
        let decision: TradeDecision =
            serde_json::from_str(&json_str).context("Failed to parse JSON as TradeDecision")?;

        // Validate the decision
        decision.validate()?;

        Ok(decision)
    }

    /// Validate the trade decision.
    fn validate(&self) -> Result<()> {
        // Check trade count
        if self.trades.len() > 5 {
            anyhow::bail!("Too many trades: {} (max 5)", self.trades.len());
        }

        // Validate each trade
        for (i, trade) in self.trades.iter().enumerate() {
            if trade.from.is_empty() {
                anyhow::bail!("Trade {}: 'from' asset is empty", i + 1);
            }
            if trade.to.is_empty() {
                anyhow::bail!("Trade {}: 'to' asset is empty", i + 1);
            }
            if trade.from == trade.to {
                anyhow::bail!(
                    "Trade {}: 'from' and 'to' are the same ({})",
                    i + 1,
                    trade.from
                );
            }
            if trade.amount_usd <= 0.0 {
                anyhow::bail!(
                    "Trade {}: amount must be positive (got {})",
                    i + 1,
                    trade.amount_usd
                );
            }
            if trade.reason.is_empty() {
                anyhow::bail!("Trade {}: reason is empty", i + 1);
            }
        }

        Ok(())
    }
}

/// Extract JSON from a potentially markdown-wrapped response.
fn extract_json(response: &str) -> Result<String> {
    let trimmed = response.trim();

    // If it starts with {, assume it's raw JSON
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }

    // Try to find JSON in markdown code blocks
    if let Some(start) = trimmed.find("```json") {
        let after_marker = &trimmed[start + 7..];
        if let Some(end) = after_marker.find("```") {
            return Ok(after_marker[..end].trim().to_string());
        }
    }

    // Try generic code block
    if let Some(start) = trimmed.find("```") {
        let after_marker = &trimmed[start + 3..];
        if let Some(end) = after_marker.find("```") {
            let content = &after_marker[..end];
            // Find the start of JSON within the code block
            if let Some(json_start) = content.find('{') {
                return Ok(content[json_start..].trim().to_string());
            }
        }
    }

    // Try to find raw JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return Ok(trimmed[start..=end].to_string());
            }
        }
    }

    anyhow::bail!("Could not find JSON in response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_raw_json() {
        let response = r#"{
            "summary": "Market looks bullish",
            "observations": [],
            "trades": []
        }"#;

        let decision = TradeDecision::parse(response).unwrap();
        assert_eq!(decision.summary, "Market looks bullish");
        assert!(decision.trades.is_empty());
    }

    #[test]
    fn test_parse_markdown_wrapped() {
        let response = r#"Here's my analysis:
```json
{
    "summary": "Test summary",
    "observations": [{"signal": "test", "confidence": "high"}],
    "trades": [{"from": "BTC", "to": "ETH", "amount_usd": 100, "reason": "test"}]
}
```"#;

        let decision = TradeDecision::parse(response).unwrap();
        assert_eq!(decision.summary, "Test summary");
        assert_eq!(decision.observations.len(), 1);
        assert_eq!(decision.trades.len(), 1);
        assert_eq!(decision.trades[0].from, "BTC");
    }

    #[test]
    fn test_parse_with_trades() {
        let response = r#"{
            "summary": "Rotating into BTC",
            "observations": [],
            "trades": [
                {"from": "USDT", "to": "BTC", "amount_usd": 1000, "reason": "Bullish momentum"}
            ]
        }"#;

        let decision = TradeDecision::parse(response).unwrap();
        assert_eq!(decision.trades.len(), 1);
        assert_eq!(decision.trades[0].from, "USDT");
        assert_eq!(decision.trades[0].to, "BTC");
        assert_eq!(decision.trades[0].amount_usd, 1000.0);
    }

    #[test]
    fn test_validate_too_many_trades() {
        let response = r#"{
            "summary": "test",
            "trades": [
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "1"},
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "2"},
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "3"},
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "4"},
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "5"},
                {"from": "A", "to": "B", "amount_usd": 100, "reason": "6"}
            ]
        }"#;

        let result = TradeDecision::parse(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Too many trades"));
    }

    #[test]
    fn test_validate_same_asset() {
        let response = r#"{
            "summary": "test",
            "trades": [
                {"from": "BTC", "to": "BTC", "amount_usd": 100, "reason": "bad"}
            ]
        }"#;

        let result = TradeDecision::parse(response);
        assert!(result.is_err());
    }
}