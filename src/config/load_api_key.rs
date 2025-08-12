use anyhow::{bail, Context, Result};
use dialoguer::console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Password;
use std::env;
use std::io::IsTerminal;

const API_KEY_ENV_VAR: &str = "MODEL_API_KEY";

/// Loads the Model API key from the environment or interactively prompts the user.
/// The key must correspond to the Model API domain you have configured.
///
/// If you do not have an API key, please obtain one from your Model API provider.
pub(crate) fn load_api_key() -> Result<String> {
    let term = Term::stderr();

    if let Ok(api_key) = env::var(API_KEY_ENV_VAR) {
        // Final concise confirmation (no secret shown)
        term.write_line(&format!(
            "{} {}",
            style("✔").green(),
            style("API key set through ENV").bold(),
        ))?;

        return Ok(api_key);
    }

    // Non-interactive context: fail clearly
    if !std::io::stdin().is_terminal() {
        bail!(
            "{} is not set and no TTY available to prompt. \
             Set it in the environment or provide a CLI flag.",
            API_KEY_ENV_VAR
        );
    }

    let api_key = prompt_for_api_key(&term).context("Failed to read the Model API key")?;

    // Final concise confirmation (no secret shown)
    term.write_line(&format!(
        "{} {}",
        style("✔").green(),
        style("API key set through CLI").bold(),
    ))?;

    Ok(api_key)
}

fn prompt_for_api_key(term: &Term) -> Result<String> {
    // Ephemeral helper block (to be cleared)
    let help = [
        format!("{}", style("API key required").bold()),
        format!("Set {} or enter it below.", style(API_KEY_ENV_VAR).cyan()),
        "The key must match your configured Model API domain.".to_string(),
    ];
    for line in &help {
        term.write_line(line)?;
    }

    // Prompt (masked)
    let api_key: String = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Model API key")
        .validate_with(|input: &String| -> std::result::Result<(), String> {
            if input.trim().is_empty() {
                Err("API key cannot be empty".into())
            } else {
                Ok(())
            }
        })
        .interact_on(&term)
        .context("Failed to read Model API key")?;

    // Clear helper + prompt (best-effort)
    term.clear_last_lines(help.len() + 1)?; // +1 for the prompt line
    Ok(api_key)
}
