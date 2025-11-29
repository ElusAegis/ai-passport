use crate::providers::Provider;
use crate::ui::spinner::with_spinner_future;
use crate::ApiProvider;
use anyhow::{Context, Error, Result};
use dialoguer::console::{style, Term};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{FuzzySelect, Input};
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::Method;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
struct Model {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ModelList {
    data: Vec<Model>,
}

/// Fetches the model list from the API and allows the user to select a model interactively.
/// Fa pub(crate)lls back to manual entry if fetching fails.
pub(crate) async fn load_model_id(api_provider: &ApiProvider) -> Result<String> {
    let fetched_model_list = with_spinner_future(
        "Waiting to load model list…",
        fetch_model_list(api_provider),
    )
    .await;

    // Convert Ok(empty) into an error so we hit the manual fallback path.
    let fetched_model_list = fetched_model_list.and_then(non_empty);

    let term = Term::stderr();

    let selected = match fetched_model_list {
        Ok(list) => prompt_from_list(list, &term)?,
        Err(error) => {
            let lines_drawn = print_ephemeral_error(error)?;

            let id = prompt_manual(&term)?;

            term.clear_last_lines(lines_drawn)?;

            id
        }
    };

    Ok(selected)
}

fn print_ephemeral_error(error: Error) -> Result<usize> {
    let error_message = [
        format!(
            "{} {}",
            style("✘").red(),
            style("Failed to fetch model list from the API.").bold(),
        ),
        format!(
            "{} {} {}",
            style(" "),
            style("The following error occurred:"),
            style(error).red()
        ),
    ];
    for line in &error_message {
        info!(target: "plain", "{}", line);
    }
    Ok(error_message.len())
}

fn non_empty<T>(v: Vec<T>) -> Result<Vec<T>> {
    if v.is_empty() {
        Err(anyhow::anyhow!("Model list is empty"))
    } else {
        Ok(v)
    }
}

fn prompt_from_list(model_list: Vec<String>, term: &Term) -> Result<String> {
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
        prompt_manual(term)?
    } else {
        options[selection].clone()
    };

    term.clear_last_lines(1)?;

    Ok(model_id)
}

async fn fetch_model_list(provider: &ApiProvider) -> Result<Vec<String>> {
    let api_domain = &provider.domain;
    let api_port = provider.port;
    let models_endpoint = provider.models_endpoint();

    let mut builder = hyper::Request::builder()
        .method(Method::GET)
        .uri(format!("https://{api_domain}:{api_port}{models_endpoint}"));

    for (name, value) in provider.models_headers() {
        builder = builder.header(name, value);
    }

    let request = builder
        .body(Empty::<Bytes>::new())
        .context("Failed to build request")?;

    let https = HttpsConnectorBuilder::new()
        .with_native_roots()? // use OS trust store
        .https_only() // HTTPS only
        .enable_http1() // we only enabled http1 in Cargo features
        .build();

    let client = Client::builder(TokioExecutor::new()).build::<_, _>(https);

    let response = client.request(request).await?;

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

fn prompt_manual(term: &Term) -> Result<String> {
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
        .interact_text_on(term)
        .context("Failed to read model ID input")?;

    term.clear_last_lines(1)?;

    Ok(manual_model_id)
}
