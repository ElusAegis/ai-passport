use crate::remote::attribution::config::ModelApiSettings;
use anyhow::{Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Select};
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
pub(crate) async fn select_model_id(api_settings: &ModelApiSettings) -> Result<String> {
    let models = fetch_model_list(api_settings).await;

    match models {
        Ok(model_list) if !model_list.is_empty() => {
            let mut options: Vec<String> = model_list;
            options.push("Enter custom model ID...".to_string());

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("🤖 Model to interact with")
                .items(&options)
                .default(0)
                .max_length(10)
                .interact()
                .context("Failed to get user selection")?;

            let model_id = if options[selection] == "Enter custom model ID..." {
                prompt_custom_model_id()?
            } else {
                options[selection].clone()
            };

            Ok(model_id)
        }
        _ => {
            println!("❌ Failed to fetch model list from the API.");
            let model_id = prompt_custom_model_id()?;
            println!("✔ selected model id: {}", model_id);
            Ok(model_id)
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
    let custom_id = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Please enter the model ID you wish to use")
        .with_initial_text("")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Model ID cannot be empty")
            } else {
                Ok(())
            }
        })
        .interact()
        .context("Failed to read model ID input")?;

    Ok(custom_id)
}
