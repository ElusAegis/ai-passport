use anyhow::{Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use std::env;

const API_DOMAIN_ENV_VAR: &str = "MODEL_API_DOMAIN";

const DEFAULT_API_DOMAIN: &str = "api.red-pill.ai"; // No default domain

/// Loads the Model API domain from the environment or interactively prompts the user.
/// The domain must not include a protocol (http:// or https://).
///
/// # Warning
/// The provided API domain must be compatible with the OpenAI API specification.
/// Using an incompatible API may result in errors or unexpected behavior.
pub(crate) fn load_api_domain() -> Result<String> {
    dotenv::dotenv().ok();

    if let Ok(api_domain) = env::var(API_DOMAIN_ENV_VAR) {
        return validate_api_domain(&api_domain);
    }

    println!("🌐 The `{API_DOMAIN_ENV_VAR}` environment variable is not set.");
    println!(
        "To interact with the models, you need to provide the domain of the Model API to target."
    );
    println!(
        "Warning: The API domain you provide must be compatible with the OpenAI API specification."
    );
    println!("If you do not have a specific API domain, please consult your service provider or administrator.");
    println!();

    let prompt = format!(
        "Enter the Model API domain [example: `{}`]:",
        DEFAULT_API_DOMAIN
    );

    let api_domain: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(&prompt)
        .default(DEFAULT_API_DOMAIN.to_string())
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Model API domain cannot be empty.")
            } else if input.trim().starts_with("http://") || input.trim().starts_with("https://") {
                Err("Do not include http:// or https://")
            } else {
                Ok(())
            }
        })
        .interact()
        .context("Failed to read Model API domain input")?;

    validate_api_domain(&api_domain)
}

fn validate_api_domain(input: &str) -> Result<String> {
    let trimmed = input.trim().trim_end_matches('/');

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        anyhow::bail!("Invalid Model API domain: do not include http:// or https://");
    }
    if trimmed.is_empty() {
        anyhow::bail!("Model API domain cannot be empty.");
    }
    Ok(trimmed.to_string())
}
