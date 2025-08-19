use anyhow::{bail, Context, Result};

const API_PORT_ENV_VAR: &str = "MODEL_API_PORT";
const DEFAULT_API_PORT: u16 = 443; // Default port for HTTPS

/// Loads the Model API port from the environment or interactively prompts the user.
/// The port must be a valid TCP port number (1-65535).
pub(crate) fn load_api_port() -> Result<u16> {
    if let Ok(port_str) = std::env::var(API_PORT_ENV_VAR) {
        let port: u16 = port_str.parse().context("Invalid port number")?;
        if port < 1 {
            bail!("Port number must be between 1 and 65535");
        }
        return Ok(port);
    }

    Ok(DEFAULT_API_PORT)
}
