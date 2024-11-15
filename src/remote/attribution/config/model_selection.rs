use crate::remote::attribution::config::ModelApiSettings;
use anyhow::{Context, Result};
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::Method;
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use std::io::Write;

pub(crate) async fn select_model_id(api_settings: &ModelApiSettings) -> Result<String> {
    loop {
        println!("🤖 Please select a model to interact with:");
        println!("1️⃣ OpenAI gpt-4o (default)");
        println!("2️⃣ Claude-3.5-Sonnet");
        println!("3️⃣ Mistral-8b");
        println!("💡 Or provide a custom model ID. Please visit `https://red-pill.ai/model-list` to view available model IDs.");
        print!("👉 Your choice: ");
        std::io::stdout()
            .flush()
            .context("Failed to flush stdout")?;

        let choice = read_user_input("selection").await?;
        let model_id = match choice.trim() {
            "1" => "gpt-4o".to_string(),
            "2" => "anthropic/claude-3-5-sonnet".to_string(),
            "3" => "mistralai/ministral-8b".to_string(),
            custom_model => {
                if validate_model_id(custom_model, api_settings).await? {
                    custom_model.to_string()
                } else {
                    println!("❌ Invalid model ID. Please enter a valid model ID from the list or provide a custom model ID.");
                    continue;
                }
            }
        };

        return Ok(model_id);
    }
}

async fn read_user_input(prompt: &str) -> Result<String> {
    print!("Please enter your {}: ", prompt);
    std::io::stdout()
        .flush()
        .context("Failed to flush stdout")?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("Failed to read user input")?;
    Ok(input.trim().to_string())
}

async fn validate_model_id(model_id: &str, api_settings: &ModelApiSettings) -> Result<bool> {
    #[derive(Debug, Deserialize)]
    struct Model {
        id: String,
    }

    #[derive(Debug, Deserialize)]
    struct ModelList {
        data: Vec<Model>,
    }

    let request = hyper::Request::builder()
        .method(Method::GET)
        .uri(format!(
            "https://{}{}",
            api_settings.server_domain, api_settings.model_list_route
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

        Ok(model_list.data.iter().any(|model| model.id == model_id))
    } else {
        eprintln!("❌ Error fetching model list: {}", response.status());
        Ok(false)
    }
}
