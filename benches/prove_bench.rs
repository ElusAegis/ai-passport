use criterion::{criterion_group, criterion_main, Criterion, SamplingMode, Throughput};
use rand::distr::Alphanumeric;
use rand::Rng;
use std::time::Duration;

use passport_for_ai::{
    get_total_sent_recv_max, run_prove, with_input_source, InputSource, ModelConfig,
    NotarisationConfig, NotaryConfig, NotaryMode, PrivacyConfig, ProveConfig, SessionMode,
};
use tlsn_common::config::NetworkSetting;

// ───────────────────────────────────────────────────────────────────────────────
// Input source (task-local DI)
// ───────────────────────────────────────────────────────────────────────────────
struct VecInputSource {
    buf: std::vec::IntoIter<Option<String>>,
}
impl VecInputSource {
    pub fn new(lines: Vec<Option<String>>) -> Self {
        Self {
            buf: lines.into_iter(),
        }
    }
}
impl InputSource for VecInputSource {
    fn next(&mut self) -> anyhow::Result<Option<String>> {
        Ok(self.buf.next().flatten())
    }
}

// ───────────────────────────────────────────────────────────────────────────────
// Presets and pairings
// ───────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum NetOpt {
    Latency,
    Bandwidth,
}

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
        // Red Pill API
        ModelPreset {
            name: "redpill-remote",
            model_id: "meta-llama/llama-3.2-1b-instruct",
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
                max_sent_bytes: 64 * KIB,
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
    ]
}

// Explicit pairing list ONLY (no cross product)
// 1) local+local
// 2) pse + redpill
fn pairings() -> Vec<(ModelPreset, NotaryPreset)> {
    let models = model_presets();
    let notaries = notary_presets();

    let poa_local = models
        .iter()
        .find(|m| m.name == "poa-local")
        .unwrap()
        .clone();
    let _redpill = models
        .iter()
        .find(|m| m.name == "redpill-remote")
        .unwrap()
        .clone();

    let notary_local = notaries
        .iter()
        .find(|n| n.name == "notary-local")
        .unwrap()
        .clone();
    let _notary_pse = notaries
        .iter()
        .find(|n| n.name == "notary-pse")
        .unwrap()
        .clone();

    vec![
        (poa_local, notary_local),
        // (redpill, notary_pse)
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
fn make_inputs(n_msgs: usize, req_bytes: usize) -> Vec<Option<String>> {
    const PACKAGE_OVERHEAD: usize = 600;

    // Ensure we have enough bytes for the request, accounting for overhead
    if req_bytes < PACKAGE_OVERHEAD {
        panic!(
            "Request size must be at least {} bytes to account for overhead.",
            PACKAGE_OVERHEAD
        );
    }

    (0..n_msgs)
        .map(|_| Some(prompt_bytes(req_bytes - PACKAGE_OVERHEAD)))
        .chain(std::iter::once(None))
        .collect()
}

// ───────────────────────────────────────────────────────────────────────────────
// Build ProveConfig (uses adjusted NotarisationConfig)
// ───────────────────────────────────────────────────────────────────────────────

fn build_prove_config(
    model: &ModelPreset,
    notary: &NotaryPreset,
    net: NetOpt,
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
    let model_list_route =
        std::env::var("MODEL_LIST_ROUTE").unwrap_or_else(|_| "/v1/models".into());

    let model_config = ModelConfig::builder()
        .domain(model.api_domain.to_string())
        .port(model.api_port)
        .inference_route(inference_route)
        .model_list_route(model_list_route)
        .api_key(api_key)
        .model_id(model.model_id.to_string())
        .build()
        .expect("model_config");

    let notary_config = NotaryConfig::builder()
        .domain(notary.domain.to_string())
        .port(notary.port)
        .path_prefix(notary.version_path.to_string())
        .mode(notary.mode)
        .build()
        .expect("notary_config");

    let notarisation_config = NotarisationConfig::builder()
        .notary_config(notary_config)
        .max_req_num_sent(max_req_num_sent)
        .max_single_request_size(max_single_request_size)
        .max_single_response_size(max_single_response_size)
        .network_optimization(match net {
            NetOpt::Latency => NetworkSetting::Latency,
            NetOpt::Bandwidth => NetworkSetting::Bandwidth,
        })
        .mode(mode)
        .build()
        .expect("notarisation_config");

    // Check if the notary can support the requested configuration
    let (max_sent, max_recv) = get_total_sent_recv_max(&notarisation_config);

    if max_sent > notary.caps.max_sent_bytes || max_recv > notary.caps.max_recv_bytes {
        return None;
    }

    Some(
        ProveConfig::builder()
            .model_config(model_config)
            .privacy_config(PrivacyConfig::default())
            .notarisation_config(notarisation_config)
            .build()
            .expect("prove_config"),
    )
}

// ───────────────────────────────────────────────────────────────────────────────
// Criterion benchmark
// ───────────────────────────────────────────────────────────────────────────────

pub fn prove_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("run_prove");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(60));

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 1), (1, 2), (2, 2), (4, 4)];

    for (model, notary) in pairings() {
        for &net in &[NetOpt::Latency, NetOpt::Bandwidth] {
            for &mode in &[SessionMode::OneShot, SessionMode::MultiRound] {
                for &(num_inputs, max_req_num) in input_cases {
                    // Base per-message sizes (your “approx” starting point)
                    let max_request_size = 1024;
                    let max_response_size = 1024;

                    // First attempt at base size; skip pair if even the base doesn’t fit
                    let cfg = match build_prove_config(
                        &model,
                        &notary,
                        net,
                        mode,
                        max_req_num,
                        max_request_size,
                        max_response_size,
                    ) {
                        Some(cfg) => cfg,
                        None => continue,
                    };

                    let bid = format!(
                        "{}+{}-{:?}-{:?}---{}(up)-{}(down)-{}(msg)-{}(max-msg)",
                        model.name,
                        notary.name,
                        net,
                        mode,
                        max_request_size,
                        max_response_size,
                        num_inputs,
                        max_req_num
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
                                let src =
                                    VecInputSource::new(make_inputs(num_inputs, max_request_size));
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

    group.finish();
}

criterion_group!(benches, prove_benchmarks);
criterion_main!(benches);
