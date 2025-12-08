//! Presets for automated benchmarking.
//!
//! Provides predefined notary, prover, and model configurations for benchmarking.

mod model;
mod notary;
mod prover;

pub use model::load_model_presets;
pub use notary::{load_notary_presets, parse_network_setting};
pub use prover::load_prover_presets;
