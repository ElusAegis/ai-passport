//! Agent core module - orchestrates the trading agent loop.
//!
//! The main entry point is [`input_source::AgentInputSource`] which implements the
//! `InputSource` trait from the CLI crate, allowing integration with
//! the prover-based architecture.

pub mod input_source;
pub mod output;
pub mod prompt;