use ai_passport::{ApiProvider, ProveConfig};
use anyhow::Context;

pub(crate) fn gen_cfg(_request_limit: usize, _response_limit: usize) -> anyhow::Result<ProveConfig> {
    let domain = "api.proof-of-autonomy.elusaegis.xyz";

    let api_provider = ApiProvider::builder()
        .domain(domain)
        .port(3000_u16)
        .api_key("secret123".to_string())
        .build()
        .context("Failed to build ApiProvider")?;

    ProveConfig::builder()
        .provider(api_provider)
        .model_id("demo-gpt-4o-mini")
        .build()
        .context("Failed to build ProveConfig")
}
