use anyhow::{Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Password;
use std::env;

const API_KEY_ENV_VAR: &str = "MODEL_API_KEY";

/// Loads the Model API key from the environment or interactively prompts the user.
/// The key must correspond to the Model API domain you have configured.
///
/// If you do not have an API key, please obtain one from your Model API provider.
pub(crate) fn load_api_key() -> Result<String> {
    if let Ok(api_key) = env::var(API_KEY_ENV_VAR) {
        return Ok(api_key);
    }

    println!("🔑 The `{API_KEY_ENV_VAR}` environment variable is not set.");
    println!("To interact with the models, you need to provide the API key for your configured Model API domain.");
    println!("If you do not have an API key, please obtain one from your Model API provider.");
    println!();

    let api_key = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Please enter your Model API key")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Model API key cannot be empty")
            } else {
                Ok(())
            }
        })
        .interact()
        .context("Failed to read Model API key input")?;

    Ok(api_key)
}
