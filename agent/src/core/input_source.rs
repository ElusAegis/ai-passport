//! Agent input source for prover integration.
//!
//! This module implements the `InputSource` trait from the CLI crate,
//! allowing the agent to integrate with the prover-based architecture.
//!
//! Instead of the user typing messages, the agent:
//! 1. Fetches data from tools (portfolio, prices, markets)
//! 2. Builds a context message for the LLM
//! 3. Parses the LLM response to extract trade decisions
//! 4. Executes trades on the portfolio

use crate::portfolio::PortfolioState;
use crate::tools::{AttestationMode, Tool, ToolOutput};
use ai_passport::{ChannelBudget, ChatMessage, InputSource, ProveConfig};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, trace};

use super::output::TradeDecision;
use super::prompt::build_system_prompt;

/// Agent input source that provides tool context as "user" messages
/// and processes LLM responses to execute trades.
pub struct AgentInputSource {
    /// Current round number (1-indexed)
    round: usize,
    /// Maximum rounds to run
    max_rounds: usize,
    /// Portfolio state (mutable for trade execution)
    portfolio: PortfolioState,
    /// Tools for fetching data
    tools: Vec<Arc<dyn Tool>>,
    /// Attestation mode for tools
    tool_attestation: AttestationMode,
    /// Optional delay between rounds
    round_delay: Option<Duration>,
    /// Whether this is the first message (needs system prompt context)
    first_message: bool,
    /// Whether we should stop after processing the last response
    should_stop: bool,
}

impl AgentInputSource {
    /// Create a new agent input source.
    pub fn new(
        portfolio: PortfolioState,
        tools: Vec<Arc<dyn Tool>>,
        max_rounds: usize,
        tool_attestation: AttestationMode,
        round_delay: Option<Duration>,
    ) -> Self {
        Self {
            round: 0,
            max_rounds,
            portfolio,
            tools,
            tool_attestation,
            round_delay,
            first_message: true,
            should_stop: false,
        }
    }

    /// Fetch data from all tools.
    async fn fetch_all_tools(&self) -> Result<Vec<ToolOutput>> {
        let mut outputs = Vec::new();

        for tool in &self.tools {
            info!("  Fetching {}...", tool.name());
            let output = tool
                .fetch(&self.tool_attestation, &self.portfolio)
                .await
                .with_context(|| format!("Tool '{}' failed", tool.name()))?;

            trace!(
                tool = tool.name(),
                fetch_time_ms = output.fetch_time_ms,
                data_size = output.data.len(),
                "Tool output data:\n{}",
                output.data
            );

            outputs.push(output);
        }

        Ok(outputs)
    }

    /// Build the context message from tool outputs.
    fn build_context_message(&self, outputs: &[ToolOutput]) -> String {
        let mut sections = Vec::new();

        for output in outputs {
            sections.push(format!(
                "## {} Data\n```json\n{}\n```",
                output.name, output.data
            ));
        }

        sections.join("\n\n")
    }

    /// Process the assistant's response, extract trades, and execute them.
    fn process_response(&mut self, response: &str) -> Result<()> {
        // Parse the trade decision
        let decision = TradeDecision::parse(response)
            .context("Failed to parse LLM response as TradeDecision")?;

        info!(
            "Round {} complete. Trades: {}",
            self.round,
            decision.trades.len()
        );

        for trade in &decision.trades {
            info!(
                "  SWAP {} -> {} ${} ({})",
                trade.from, trade.to, trade.amount_usd, trade.reason
            );
        }

        // Execute trades on portfolio
        for trade in &decision.trades {
            if let Err(e) = self
                .portfolio
                .execute_swap(&trade.from, &trade.to, trade.amount_usd)
            {
                tracing::warn!("Trade execution failed: {}", e);
            }
        }

        // Log portfolio state after trades
        if !decision.trades.is_empty() {
            info!("Portfolio state after trades:");
            for line in self.portfolio.format_detailed().lines() {
                info!("{}", line);
            }
        }

        Ok(())
    }

    /// Log the current portfolio state at INFO level.
    fn log_portfolio_state(&self) {
        for line in self.portfolio.format_detailed().lines() {
            info!("{}", line);
        }
    }
}

impl InputSource for AgentInputSource {
    fn next_message(
        &mut self,
        _budget: &ChannelBudget,
        _config: &ProveConfig,
        past_messages: &[ChatMessage],
    ) -> anyhow::Result<Option<ChatMessage>> {
        // If we should stop, return None
        if self.should_stop {
            info!("Final portfolio state:");
            self.log_portfolio_state();
            return Ok(None);
        }

        // Process the previous assistant response (if any)
        if let Some(last_msg) = past_messages.last() {
            // The last message should be from the assistant
            trace!(
                response_size = last_msg.content().len(),
                "LLM response:\n{}",
                last_msg.content()
            );

            if let Err(e) = self.process_response(last_msg.content()) {
                tracing::error!("Failed to process response: {}", e);
                // Continue anyway - don't stop the loop
            }

            // Check if we've completed all rounds
            if self.round >= self.max_rounds {
                self.should_stop = true;
                // We need to signal stop, but also allow the final response processing
                // Return None to end the loop
                info!("Final portfolio state:");
                self.log_portfolio_state();
                return Ok(None);
            }

            // Apply delay between rounds (if configured)
            if let Some(delay) = self.round_delay {
                info!("Waiting {:?} before next round...", delay);
                // Note: This is blocking in the sync context of InputSource
                // For proper async support, we'd need to refactor the InputSource trait
                std::thread::sleep(delay);
            }
        }

        // Increment round counter
        self.round += 1;

        info!("═══════════════════════════════════════════════════════════");
        info!("Round {}/{}", self.round, self.max_rounds);
        info!("═══════════════════════════════════════════════════════════");

        // Debug log portfolio state at start of round
        debug!("Portfolio state at start of round {}:", self.round);
        debug!("\n{}", self.portfolio);

        // Fetch tool data (we need to block on the async operation)
        // This is a limitation of the sync InputSource trait
        let tool_outputs = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.fetch_all_tools())
        })?;

        // Build context message
        let mut context_msg = self.build_context_message(&tool_outputs);

        // For the first message, prepend the system prompt context
        if self.first_message {
            self.first_message = false;
            // Log initial state
            info!("Initial portfolio state:");
            self.log_portfolio_state();

            // Add system prompt as a preamble
            let system_prompt = build_system_prompt();
            context_msg = format!(
                "SYSTEM INSTRUCTIONS:\n{}\n\n---\n\nCURRENT MARKET DATA:\n{}",
                system_prompt, context_msg
            );
        }

        info!("Context message size: {} bytes", context_msg.len());

        Ok(Some(ChatMessage::user(context_msg)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_input_source_creation() {
        let portfolio = PortfolioState::sample();
        let tools: Vec<Arc<dyn Tool>> = vec![];
        let source = AgentInputSource::new(portfolio, tools, 3, AttestationMode::Direct, None);

        assert_eq!(source.round, 0);
        assert_eq!(source.max_rounds, 3);
        assert!(!source.should_stop);
    }
}
