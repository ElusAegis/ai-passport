use anyhow::{Context, Result};
use std::env;
use std::io::Write;

const API_KEY_ENV_VAR: &str = "REDPIL_API_KEY";

pub(crate) fn load_api_key() -> Result<String> {
    dotenv::dotenv().ok();

    if let Ok(api_key) = env::var(API_KEY_ENV_VAR) {
        return Ok(api_key);
    }

    // Prompt the user to enter the API key if not set
    println!("🔑 The `{API_KEY_ENV_VAR}` environment variable is not set.");
    println!("To interact with the models, you need to provide the API key.");
    println!("If you do not have an API key, you can sign up for one at:");
    println!("`https://red-pill.ai/keys`");
    print!("Please now enter your Red Pill API key: ");
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout")?;

    // Capture user input for the API key
    let mut api_key_input = String::new();
    std::io::stdin()
        .read_line(&mut api_key_input)
        .context("Failed to read user API key input")?;
    let api_key = api_key_input.trim().to_string();

    Ok(api_key)
}
