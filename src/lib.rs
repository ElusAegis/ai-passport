mod app;
mod args;
mod config;
mod prove;
mod utils;
mod verify;

pub use app::Application;
pub use config::{ModelConfig, NotaryConfig, PrivacyConfig, ProveConfig};
pub use prove::run_prove;
pub use utils::io_input::{with_input_source, InputSource};
