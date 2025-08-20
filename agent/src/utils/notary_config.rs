use ai_passport::{
    ModelConfig, NetworkSetting, NotaryConfig, NotaryMode, PrivacyConfig, ProveConfig,
    ServerConfig, SessionConfig, SessionMode,
};
use anyhow::Context;

pub(crate) fn gen_cfg(request_limit: usize, response_limit: usize) -> anyhow::Result<ProveConfig> {
    let server_config = ServerConfig::builder()
        .domain("api.proof-of-autonomy.elusaegis.xyz".to_string())
        .port(3000_u16)
        .build()
        .expect("server_config");

    let model_config = ModelConfig::builder()
        .server(server_config)
        .inference_route("/v1/chat/completions".to_string())
        .api_key("secret123".to_string())
        .model_id("demo-gpt-4o-mini")
        .build()
        .expect("model_config");

    let session_config = SessionConfig::builder()
        .max_msg_num(1)
        .max_single_request_size(request_limit)
        .max_single_response_size(response_limit)
        .mode(SessionMode::Single)
        .build()
        .expect("session_config");

    let notary_config = NotaryConfig::builder()
        .domain("localhost".to_string())
        .port(7047_u16)
        .path_prefix("".to_string())
        .mode(NotaryMode::RemoteNonTLS)
        .network_optimization(NetworkSetting::Latency)
        .finalize_for_session(&session_config)
        .expect("notary_config");

    ProveConfig::builder()
        .model(model_config)
        .notary(notary_config)
        .session(session_config)
        .privacy(PrivacyConfig::default())
        .build()
        .context("notary_config")
}
