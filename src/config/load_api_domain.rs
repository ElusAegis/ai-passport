use anyhow::{bail, Context, Result};
use dialoguer::console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use std::env;
use std::io::IsTerminal;

const API_DOMAIN_ENV_VAR: &str = "MODEL_API_DOMAIN";

const DEFAULT_API_DOMAIN: &str = "api.red-pill.ai"; // No default domain

/// Loads the Model API domain from the environment or interactively prompts the user.
/// The domain must not include a protocol (http:// or https://).
///
/// # Warning
/// The provided API domain must be compatible with the OpenAI API specification.
/// Using an incompatible API may result in errors or unexpected behavior.
pub(crate) fn load_api_domain() -> Result<String> {
    // Use stderr for ephemeral UI; keeps stdout clean for piping
    let term = Term::stderr();

    if let Ok(api_domain) = env::var(API_DOMAIN_ENV_VAR) {
        validate_api_domain(&api_domain)?;

        let label = "Using set Model API domain";
        let summary = format!(
            "{} {} · {}",
            style("✔").green(),
            style(label).bold(),
            api_domain
        );
        term.write_line(&summary)?;

        return Ok(api_domain);
    }

    // Non-interactive context: fail clearly
    if !std::io::stdin().is_terminal() {
        bail!(
            "{} is not set and no TTY available to prompt. \
             Set it in the environment or provide a CLI flag.",
            API_DOMAIN_ENV_VAR
        );
    }

    let api_domain =
        prompt_for_api_domain(&term).context("Failed to select the Model API domain")?;

    let label = "Selected Model API domain";
    let summary = format!(
        "{} {} · {}",
        style("✔").green(),
        style(label).bold(), // make the label bold
        api_domain
    );
    term.write_line(&summary)?;

    Ok(api_domain)
}

fn prompt_for_api_domain(term: &Term) -> Result<String> {
    // Ephemeral help block
    let help = [
        format!("{}", style("API domain required").bold()),
        format!(
            "Set {} or enter a domain below (no scheme). Example: {}",
            style(API_DOMAIN_ENV_VAR).cyan(),
            DEFAULT_API_DOMAIN
        ),
        "The domain must be OpenAI-compatible.".to_string(),
    ];
    for line in &help {
        term.write_line(line)?;
    }

    // Prompt on the same terminal, with validation
    let api_domain: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Model API domain")
        .default(DEFAULT_API_DOMAIN.to_string())
        .validate_with(|input: &String| -> std::result::Result<(), String> {
            validate_api_domain(input).map_err(|e| e.to_string())
        })
        .interact_text_on(&term)
        .context("Failed to read Model API domain")?;

    // Remove the help + prompt line from the screen
    // (If your theme prints extra lines, bump this by 1.)
    term.clear_last_lines(help.len() + 1)?;
    Ok(api_domain)
}

pub fn validate_api_domain(input: &String) -> Result<()> {
    let s = input.trim();

    if s.is_empty() {
        bail!("Model API domain cannot be empty.");
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        bail!("Invalid Model API domain: do not include http:// or https://");
    }
    // Disallow any path, query, or fragment
    if s.contains('/') || s.contains('?') || s.contains('#') {
        bail!("Provide only a domain (optionally :port), e.g., api.example.com or localhost:8080 — no paths like /v1.");
    }
    // Optional: reject whitespace inside
    if s.split_whitespace().count() != 1 {
        bail!("Domain must not contain spaces or tabs.");
    }

    Ok(())
}
