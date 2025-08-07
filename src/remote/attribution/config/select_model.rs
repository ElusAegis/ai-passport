use crate::remote::attribution::config::ModelApiSettings;
use anyhow::{Context, Result};
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::Method;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use inquire::{Select, Text};
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
pub(crate) async fn select_model_id(api_settings: &ModelApiSettings) -> Result<String> {
    let models = fetch_model_list(api_settings).await;

    match models {
        Ok(model_list) if !model_list.is_empty() => {
            let mut options: Vec<String> = model_list;
            options.push("Enter custom model ID...".to_string());

            let selection = Select::new(
                "🤖 Please select a model to interact with (scroll or type to search):",
                options.clone(),
            )
            .with_page_size(10)
            .prompt();

            match selection {
                Ok(choice) if choice == "Enter custom model ID..." => prompt_custom_model_id(),
                Ok(choice) => Ok(choice),
                Err(_) => prompt_custom_model_id(),
            }
        }
        _ => {
            println!("❌ Failed to fetch model list from the API.");
            prompt_custom_model_id()
        }
    }
}

async fn fetch_model_list(api_settings: &ModelApiSettings) -> Result<Vec<String>> {
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

fn prompt_custom_model_id() -> Result<String> {
    let custom_id = Text::new("Please enter the model ID you wish to use:")
        .with_help_message(
            "You can find available model IDs in your provider's documentation or dashboard.",
        )
        .prompt()
        .context("Failed to read model ID input")?;
    if custom_id.trim().is_empty() {
        anyhow::bail!("Model ID cannot be empty.");
    }
    Ok(custom_id)
}
