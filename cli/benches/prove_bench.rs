use ai_passport::{
    run_prove, with_input_source, ApiProvider, ModelConfig, NotaryConfig, NotaryMode, ProveConfig,
    ServerConfig, SessionConfig, SessionMode, VecInputSource,
};
use criterion::measurement::WallTime;
use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, Criterion, SamplingMode, Throughput,
};
use rand::distr::Alphanumeric;
use rand::Rng;
use std::time::Duration;
use tlsn_common::config::NetworkSetting;

fn ensure_tracing() {
    use tracing_subscriber::EnvFilter;
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_line_number(true)
            .with_file(true)
            .try_init();
    });
}

// ───────────────────────────────────────────────────────────────────────────────
// Presets and pairings
// ───────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ModelPreset {
    name: &'static str,
    model_id: &'static str,
    api_domain: &'static str,
    api_port: u16,
    api_key_source: ApiKeySource,
}

#[derive(Clone)]
enum ApiKeySource {
    Direct(&'static str),
    FromEnv(&'static str), // read from .env / env
}

#[derive(Clone)]
struct NotaryPreset {
    name: &'static str,
    domain: &'static str,
    port: u16,
    version_path: &'static str, // "" if none
    mode: NotaryMode,
    caps: NotaryCaps,
}

#[derive(Clone, Copy)]
struct NotaryCaps {
    max_sent_bytes: usize,
    max_recv_bytes: usize,
}

// Models
fn model_presets() -> Vec<ModelPreset> {
    vec![
        // Local PoA API
        ModelPreset {
            name: "poa-local",
            model_id: "openai/gpt-4o-mini",
            api_domain: "api.proof-of-autonomy.elusaegis.xyz",
            api_port: 3000,
            api_key_source: ApiKeySource::Direct("secret123"),
        },
        // Red Pill API (3b)
        ModelPreset {
            name: "redpill-remote-3b",
            model_id: "mistralai/ministral-3b",
            api_domain: "api.red-pill.ai",
            api_port: 443,
            api_key_source: ApiKeySource::FromEnv("REDPILL_API_KEY"),
        },
        // Red Pill API
        ModelPreset {
            name: "redpill-remote-8b",
            model_id: "mistralai/ministral-8b",
            api_domain: "api.red-pill.ai",
            api_port: 443,
            api_key_source: ApiKeySource::FromEnv("REDPILL_API_KEY"),
        },
    ]
}

// Notaries
fn notary_presets() -> Vec<NotaryPreset> {
    const KIB: usize = 1024;
    vec![
        // Local notary: sent=64 MiB, recv=16 MiB
        NotaryPreset {
            name: "notary-local",
            domain: "localhost",
            port: 7047,
            version_path: "",
            mode: NotaryMode::RemoteNonTLS,
            caps: NotaryCaps {
                max_sent_bytes: 16 * KIB,
                max_recv_bytes: 16 * KIB,
            },
        },
        // My remote: sent=16 KiB, recv=16 KiB
        NotaryPreset {
            name: "notary-remote",
            domain: "notary.proof-of-autonomy.elusaegis.xyz",
            port: 7047,
            version_path: "",
            mode: NotaryMode::RemoteTLS,
            caps: NotaryCaps {
                max_sent_bytes: 16 * KIB,
                max_recv_bytes: 16 * KIB,
            },
        },
        // PSE notary: sent=4 MiB, recv=16 MiB
        NotaryPreset {
            name: "notary-pse",
            domain: "notary.pse.dev",
            port: 443,
            version_path: "v0.1.0-alpha.12",
            mode: NotaryMode::RemoteTLS,
            caps: NotaryCaps {
                max_sent_bytes: 4 * KIB,
                max_recv_bytes: 16 * KIB,
            },
        },
        // PSE notary: sent=4 MiB, recv=16 MiB
        NotaryPreset {
            name: "notary-pse-tee",
            domain: "notary.pse.dev",
            port: 443,
            version_path: "v0.1.0-alpha.12-sgx",
            mode: NotaryMode::RemoteTLS,
            caps: NotaryCaps {
                max_sent_bytes: 4 * KIB,
                max_recv_bytes: 16 * KIB,
            },
        },
    ]
}

// Explicit pairing list ONLY (no cross product)
// 1) local+local
// 2) pse + redpill
#[allow(unused)]
fn standard_pairings() -> Vec<(ModelPreset, NotaryPreset)> {
    let models = model_presets();
    let notaries = notary_presets();

    let redpill_3b = models
        .iter()
        .find(|m| m.name == "redpill-remote-3b")
        .unwrap()
        .clone();
    let redpill_8b = models
        .iter()
        .find(|m| m.name == "redpill-remote-8b")
        .unwrap()
        .clone();

    let notary_local = notaries
        .iter()
        .find(|n| n.name == "notary-local")
        .unwrap()
        .clone();
    let notary_remote = notaries
        .iter()
        .find(|n| n.name == "notary-remote")
        .unwrap()
        .clone();
    let notary_pse = notaries
        .iter()
        .find(|n| n.name == "notary-pse")
        .unwrap()
        .clone();

    vec![
        (redpill_3b.clone(), notary_remote.clone()),
        (redpill_3b, notary_pse),
    ]
}

// Explicit pairing list ONLY (no cross product)
// 1) local+local
// 2) pse + redpill
fn model_comparison_pairings() -> Vec<(ModelPreset, NotaryPreset)> {
    let models = model_presets();
    let notaries = notary_presets();

    let redpill_3b = models
        .iter()
        .find(|m| m.name == "redpill-remote-3b")
        .unwrap()
        .clone();
    let redpill_8b = models
        .iter()
        .find(|m| m.name == "redpill-remote-8b")
        .unwrap()
        .clone();

    let notary_remote = notaries
        .iter()
        .find(|n| n.name == "notary-remote")
        .unwrap()
        .clone();
    let notary_pse = notaries
        .iter()
        .find(|n| n.name == "notary-pse")
        .unwrap()
        .clone();

    vec![
        (redpill_8b.clone(), notary_remote.clone()),
        (redpill_3b.clone(), notary_pse.clone()),
        (redpill_8b, notary_pse),
        (redpill_3b, notary_remote),
    ]
}

// Explicit pairing list ONLY (no cross product)
// 1) local+local
// 2) pse + redpill
fn notary_comparison_pairings() -> Vec<(ModelPreset, NotaryPreset)> {
    let models = model_presets();
    let notaries = notary_presets();

    let redpill_3b = models
        .iter()
        .find(|m| m.name == "redpill-remote-3b")
        .unwrap()
        .clone();

    let notary_remote = notaries
        .iter()
        .find(|n| n.name == "notary-remote")
        .unwrap()
        .clone();
    let notary_pse = notaries
        .iter()
        .find(|n| n.name == "notary-pse")
        .unwrap()
        .clone();
    let notary_pse_tee = notaries
        .iter()
        .find(|n| n.name == "notary-pse-tee")
        .unwrap()
        .clone();

    vec![
        (redpill_3b.clone(), notary_remote),
        (redpill_3b.clone(), notary_pse),
        (redpill_3b, notary_pse_tee),
    ]
}

// ───────────────────────────────────────────────────────────────────────────────
// Sizing helpers and capacity fitting
// ───────────────────────────────────────────────────────────────────────────────

/// Generate ~X bytes ASCII payload (approx).
fn prompt_bytes(n: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// N prompts (of ~req_bytes) then a terminating None (exit).
fn make_inputs(n_msgs: usize, req_bytes: usize) -> anyhow::Result<Vec<Option<String>>> {
    let inputs = (0..n_msgs)
        .map(|_| Some(prompt_bytes(req_bytes)))
        .chain(std::iter::once(None))
        .collect();

    Ok(inputs)
}

// ───────────────────────────────────────────────────────────────────────────────
// Build ProveConfig (uses adjusted NotarisationConfig)
// ───────────────────────────────────────────────────────────────────────────────

fn build_prove_config(
    model: &ModelPreset,
    notary: &NotaryPreset,
    mode: SessionMode,
    max_req_num_sent: usize,
    max_single_request_size: usize,
    max_single_response_size: usize,
) -> Option<ProveConfig> {
    // Load .env once (for REDPILL_API_KEY etc.)
    let _ = dotenvy::dotenv();

    let api_key = match &model.api_key_source {
        ApiKeySource::Direct(v) => (*v).to_string(),
        ApiKeySource::FromEnv(name) => std::env::var(name).expect("Missing API key env var"),
    };

    let inference_route =
        std::env::var("MODEL_INFER_ROUTE").unwrap_or_else(|_| "/v1/chat/completions".into());

    let server_config = ServerConfig::builder()
        .domain(model.api_domain.to_string())
        .port(model.api_port)
        .build()
        .expect("server_config");

    let model_config = ModelConfig::builder()
        .server(server_config)
        .inference_route(inference_route)
        .api_key(api_key)
        .model_id(model.model_id.to_string())
        .build()
        .expect("model_config");

    let session_config = SessionConfig::builder()
        .max_msg_num(max_req_num_sent)
        .max_single_request_size(max_single_request_size)
        .max_single_response_size(max_single_response_size)
        .mode(mode)
        .build()
        .expect("session_config");

    let notary_config = NotaryConfig::builder()
        .domain(notary.domain.to_string())
        .port(notary.port)
        .path_prefix(notary.version_path.to_string())
        .mode(notary.mode)
        .network_optimization(NetworkSetting::Latency)
        .finalize_for_session(&session_config)
        .expect("notary_config");

    let (max_total_sent, max_total_recv) = session_config.max_total_sent_recv();

    // Check if the notary can support the requested configuration
    if max_total_sent > notary.caps.max_sent_bytes || max_total_recv > notary.caps.max_recv_bytes {
        return None;
    }

    let provider = ApiProvider::from_domain(model.api_domain);

    Some(
        ProveConfig::builder()
            .model(model_config)
            .privacy(provider.into())
            .notary(notary_config)
            .session(session_config)
            .build()
            .expect("prove_config"),
    )
}

// ───────────────────────────────────────────────────────────────────────────────
// Criterion benchmark
// ───────────────────────────────────────────────────────────────────────────────

pub fn optimized_regular_benchmark_known_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("benches_known_conv_size");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    // Initiate logger from tracing using env and error as backup
    ensure_tracing();

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 1), (2, 2), (4, 4), (8, 8)];
    let modes = &[SessionMode::Single, SessionMode::Multi];

    run_cases(&mut group, input_cases, modes, standard_pairings());

    group.finish();
}

pub fn optimized_regular_benchmark_unknown_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("benches_unknown_conv_size");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    // Initiate logger from tracing using env and error as backup
    ensure_tracing();

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 4), (2, 4), (3, 4), (4, 4)];
    let modes = &[SessionMode::Single, SessionMode::Multi];

    run_cases(&mut group, input_cases, modes, standard_pairings());

    group.finish();
}

pub fn model_comparison_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("benches_models");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    // Initiate logger from tracing using env and error as backup
    ensure_tracing();

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 1), (2, 2), (4, 4)];
    let modes = &[SessionMode::Single];

    run_cases(&mut group, input_cases, modes, model_comparison_pairings());

    group.finish();
}

pub fn notary_comparison_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("benches_notaries");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));

    // Initiate logger from tracing using env and error as backup
    ensure_tracing();

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 1), (2, 2), (4, 4)];
    let modes = &[SessionMode::Single];

    run_cases(&mut group, input_cases, modes, notary_comparison_pairings());

    group.finish();
}

fn run_cases(
    group: &mut BenchmarkGroup<WallTime>,
    input_cases: &[(usize, usize)],
    modes: &[SessionMode],
    pairings: Vec<(ModelPreset, NotaryPreset)>,
) {
    for &(num_inputs, max_req_num) in input_cases {
        for (model, notary) in &pairings {
            for &mode in modes {
                // Base per-message sizes (your “approx” starting point)
                let max_request_size = 500;
                let max_response_size = 1000;

                // First attempt at base size; skip pair if even the base doesn’t fit
                let cfg = match build_prove_config(
                    model,
                    notary,
                    mode,
                    max_req_num,
                    max_request_size,
                    max_response_size,
                ) {
                    Some(cfg) => cfg,
                    None => continue,
                };

                let Ok(input) = make_inputs(num_inputs, max_request_size) else {
                    continue;
                };

                let (_max_total_sent, _max_total_recv) = cfg.session.max_total_sent_recv();

                let bid = format!(
                    "{}+{}-{:?}---{}(#msg)-{}(#max-msg)",
                    model.name, notary.name, mode, num_inputs, max_req_num
                );
                group.throughput(Throughput::Elements(num_inputs as u64));

                // Single-threaded runtime to keep overhead predictable
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                group.bench_with_input(bid.clone(), &num_inputs, |b, &_| {
                    b.iter(|| {
                        rt.block_on(async {
                            let src = VecInputSource::new(input.clone());
                            with_input_source(src, async {
                                let _ = run_prove(&cfg).await.map_err(|e| {
                                    println!(
                                        "Failed to run bid {bid:?}. Error in run_prove: {e:?}."
                                    )
                                });
                            })
                            .await;
                        });
                    });
                });
            }
        }
    }
}

criterion_group!(benches_known, optimized_regular_benchmark_known_size);
criterion_group!(benches_unknown, optimized_regular_benchmark_unknown_size);
criterion_group!(benches_models, model_comparison_benchmark);
criterion_group!(benches_notaries, notary_comparison_benchmark);

criterion_main!(
    benches_known,
    benches_unknown,
    benches_models,
    benches_notaries
);
