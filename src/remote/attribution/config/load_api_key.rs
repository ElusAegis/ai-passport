use anyhow::{Context, Result};
use inquire::Password;
use std::env;

const API_KEY_ENV_VAR: &str = "MODEL_API_KEY";

/// Loads the Model API key from the environment or interactively prompts the user.
/// The key must correspond to the Model API domain you have configured.
///
/// If you do not have an API key, please obtain one from your Model API provider.
pub(crate) fn load_api_key() -> Result<String> {
    dotenv::dotenv().ok();

    if let Ok(api_key) = env::var(API_KEY_ENV_VAR) {
        return Ok(api_key);
    }

    println!("🔑 The `{API_KEY_ENV_VAR}` environment variable is not set.");
    println!("To interact with the models, you need to provide the API key for your configured Model API domain.");
    println!("If you do not have an API key, please obtain one from your Model API provider.");
    println!();

    let api_key = Password::new("Please enter your Model API key:")
        .without_confirmation()
        .prompt()
        .context("Failed to read Model API key input")?;

    if api_key.trim().is_empty() {
        anyhow::bail!("Model API key cannot be empty.");
    }

    Ok(api_key)
}
