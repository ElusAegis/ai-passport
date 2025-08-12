use crate::config::ModelConfig;
use crate::utils::spinner::with_spinner_future;
use anyhow::{Context, Result};
use dialoguer::console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{FuzzySelect, Input};
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::Method;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Model {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ModelList {
    data: Vec<Model>,
}

/// Fetches the model list from the API and allows the user to select a model interactively.
/// Falls back to manual entry if fetching fails.
pub(crate) async fn select_model_id(api_settings: &ModelConfig) -> Result<String> {
    let model_list = with_spinner_future(
        "Waiting to load model list…",
        fetch_model_list(api_settings),
    )
    .await
    .unwrap_or(vec![]);

    let term = Term::stderr();

    let model_id = if !model_list.is_empty() {
        prompt_for_model_id_from_list(model_list, &term)?
    } else {
        let summary = format!(
            "{} {}",
            style("✘").red(),
            style("Failed to fetch model list from the API.").bold(),
        );
        term.write_line(&summary)?;

        prompt_for_manual_model_id(&term)?
    };

    let label = "Selected Model ID";
    let summary = format!(
        "{} {} · {}",
        style("✔").green(),
        style(label).bold(),
        model_id
    );
    term.write_line(&summary)?;

    Ok(model_id)
}

fn prompt_for_model_id_from_list(model_list: Vec<String>, term: &Term) -> Result<String> {
    let mut options: Vec<String> = model_list;
    options.push("Enter model ID manually...".to_string());

    let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Model to interact with (type to filter)")
        .items(&options)
        .default(0)
        .max_length(10)
        .interact()
        .context("Failed to get user selection")?;

    let model_id = if options[selection] == "Enter custom model ID..." {
        prompt_for_manual_model_id(term)?
    } else {
        options[selection].clone()
    };

    term.clear_last_lines(1)?;

    Ok(model_id)
}

async fn fetch_model_list(api_settings: &ModelConfig) -> Result<Vec<String>> {
    let request = hyper::Request::builder()
        .method(Method::GET)
        .uri(format!(
            "https://{}{}",
            api_settings.domain, api_settings.model_list_route
        ))
        .body(Empty::<Bytes>::new())
        .context("Failed to build request")?;

    let https = HttpsConnector::new();
    let client = Client::builder(TokioExecutor::new()).build::<_, _>(https);

    let response = client
        .request(request)
        .await
        .context("Failed to send request to API")?;

    if response.status().is_success() {
        let body = response
            .into_body()
            .collect()
            .await
            .context("Failed to read response body")?
            .to_bytes();
        let model_list: ModelList =
            serde_json::from_slice(&body).context("Failed to deserialize model list")?;
        Ok(model_list.data.into_iter().map(|m| m.id).collect())
    } else {
        Err(anyhow::anyhow!(
            "Error fetching model list: {}",
            response.status()
        ))
    }
}

fn prompt_for_manual_model_id(term: &Term) -> Result<String> {
    let manual_model_id = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Manually enter desired Model ID")
        .with_initial_text("")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Model ID cannot be empty")
            } else {
                Ok(())
            }
        })
        .interact_text_on(&term)
        .context("Failed to read model ID input")?;

    term.clear_last_lines(2)?;

    Ok(manual_model_id)
}
