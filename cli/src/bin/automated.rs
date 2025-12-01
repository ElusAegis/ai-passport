use ai_passport::{
    with_input_source, AgentProver, ApiProvider, DirectProver, ProveConfig, Prover, VecInputSource,
};
use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let _ = dotenvy::dotenv().ok();

    let api_key = dotenvy::var("MODEL_API_KEY").context("MODEL_API_KEY must be set")?;
    let domain = dotenvy::var("MODEL_API_DOMAIN").context("MODEL_API_DOMAIN must be set")?;
    let port = dotenvy::var("MODEL_API_PORT")
        .map(|port| port.parse::<u16>())
        .unwrap_or(Ok(443))?;

    let api_provider = ApiProvider::builder()
        .domain(domain)
        .port(port)
        .api_key(api_key)
        .build()
        .context("Failed to build ApiProvider")?;

    let config = ProveConfig {
        provider: api_provider,
        model_id: "demo-gpt-4o-mini".to_string(),
    };

    let prover = AgentProver::Direct(DirectProver {});

    let input = vec![
        "Hello, world!".to_string(),
        "This is a test of the automated testing script.".to_string(),
    ];

    let input_source = VecInputSource::new(input);

    with_input_source(input_source, prover.run(&config)).await
}
