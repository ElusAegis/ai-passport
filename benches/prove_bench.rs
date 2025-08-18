use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};

use passport_for_ai::{
    run_prove, with_input_source, InputSource, ModelConfig, NotarisationConfig, NotaryConfig,
    NotaryMode, PrivacyConfig, ProveConfig, SessionMode,
};

use rand::distr::Alphanumeric;
use rand::Rng;
use tlsn_common::config::NetworkSetting;
// ───────────────────────────────────────────────────────────────────────────────
// Helpers
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

#[derive(Clone, Copy, Debug)]
enum NetOpt {
    Latency,
    Bandwidth,
}

#[derive(Clone)]
struct ModelPreset {
    name: &'static str,
    model_id: &'static str,
}

fn model_presets() -> Vec<ModelPreset> {
    vec![
        ModelPreset {
            name: "gpt-4o-mini",
            model_id: "openai/gpt-4o-mini",
        },
        ModelPreset {
            name: "llama-1b",
            model_id: "meta-llama/llama-3.2-1b-instruct",
        },
        ModelPreset {
            name: "llama-90b-vision",
            model_id: "meta-llama/llama-3.2-90b-vision-instruct",
        },
        ModelPreset {
            name: "llama-11b-vision",
            model_id: "meta-llama/llama-3.2-11b-vision-instruct",
        },
    ]
}

/// Generate ~2 KB ASCII prompt. Tweak if your stack adds overhead per message.
fn prompt_2kb() -> String {
    let n = 2000;
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

/// Build ProveConfig from knobs; domain/port/routes/api_key come from env to avoid secrets in code.
fn build_config(
    model: &ModelPreset,
    net: NetOpt,
    mode: SessionMode,
    max_req_num_sent: usize,
) -> ProveConfig {
    let domain = std::env::var("MODEL_DOMAIN").unwrap_or_else(|_| "api.openai.com".to_string());
    let port = std::env::var("MODEL_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(443u16);
    let api_key = std::env::var("MODEL_API_KEY").unwrap_or_else(|_| "DUMMY".into());
    let inference_route =
        std::env::var("MODEL_INFER_ROUTE").unwrap_or_else(|_| "/v1/chat/completions".into());
    let model_list_route =
        std::env::var("MODEL_LIST_ROUTE").unwrap_or_else(|_| "/v1/models".into());

    let model_config = ModelConfig::builder()
        .domain(domain)
        .port(port)
        .inference_route(inference_route)
        .model_list_route(model_list_route)
        .api_key(api_key)
        .model_id(model.model_id.to_string())
        .build()
        .expect("model_config");

    let privacy_config = PrivacyConfig::default();

    let notary_config = NotaryConfig::builder()
        .port(7074u16)
        .domain("localhost".to_string())
        .path_prefix("v0.1.0-alpha.12")
        .mode(NotaryMode::RemoteNonTLS)
        .build()
        .expect("notary_config");

    let notarisation_config = NotarisationConfig::builder()
        .notary_config(notary_config)
        .max_req_num_sent(max_req_num_sent)
        .max_single_request_size(1024) // sensible ceilings; adjust to your envelope
        .max_single_response_size(1024)
        .network_optimization(match net {
            NetOpt::Latency => NetworkSetting::Latency,
            NetOpt::Bandwidth => NetworkSetting::Bandwidth,
        })
        .mode(mode)
        .build()
        .expect("notary_config");

    ProveConfig::builder()
        .model_config(model_config)
        .privacy_config(privacy_config)
        .notarisation_config(notarisation_config)
        .build()
        .expect("prove_config")
}

/// N prompts then a terminating None (empty carriage / exit).
fn make_inputs(n: usize) -> Vec<Option<String>> {
    (0..n)
        .map(|_| Some(prompt_2kb()))
        .chain(std::iter::once(None))
        .collect()
}

// ───────────────────────────────────────────────────────────────────────────────
// Criterion benchmark
// ───────────────────────────────────────────────────────────────────────────────

pub fn prove_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("run_prove");
    // Networked benchmarks tend to be noisy; flatten sampling and increase window.
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(15); // tune up/down based on your run times
    group.measurement_time(Duration::from_secs(25)); // per-case budget

    // Input batch sizes and max-req constraints (align 5 and 10 as you requested)
    let input_cases: &[(usize, usize)] = &[(1, 1), (2, 2), (3, 3), (5, 5), (10, 10)];

    // Sweep the full grid for the default model; use a representative case for others.
    let presets = model_presets();
    let default = &presets[0];

    // Full sweep on default model
    for &net in &[NetOpt::Latency, NetOpt::Bandwidth] {
        for &mode in &[SessionMode::OneShot, SessionMode::MultiRound] {
            for &(num_inputs, max_req) in input_cases {
                let cfg = build_config(default, net, mode, max_req);
                let bid = BenchmarkId::new(
                    format!("{}-{:?}-{:?}", default.name, net, mode),
                    format!("inputs={}", num_inputs),
                );
                group.throughput(Throughput::Elements(num_inputs as u64));
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                group.bench_with_input(bid, &num_inputs, |b, &_| {
                    b.iter(|| {
                        rt.block_on(async {
                            let src = VecInputSource::new(make_inputs(num_inputs));
                            // Inject src into task-local context for the entire run
                            with_input_source(src, async {
                                // run_prove will discover the source via task-local,
                                // and will stop when it reads the injected terminator (None/empty).
                                let _ = std::hint::black_box(run_prove(&cfg)).await;
                            })
                            .await;
                        });
                    });
                });
            }
        }
    }

    // Representative case for non-default models to keep total runtime sane.
    // Pick "5 inputs" as representative; adjust if you prefer "3" or "10".
    let repr = (5usize, 5usize);
    for model in presets.iter().skip(1) {
        for &net in &[NetOpt::Latency, NetOpt::Bandwidth] {
            for &mode in &[SessionMode::OneShot, SessionMode::MultiRound] {
                let (num_inputs, max_req) = repr;
                let cfg = build_config(model, net, mode, max_req);
                let bid = BenchmarkId::new(
                    format!("{}-{:?}-{:?}", model.name, net, mode),
                    format!("inputs={}", num_inputs),
                );
                group.throughput(Throughput::Elements(num_inputs as u64));
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();

                group.bench_with_input(bid, &num_inputs, |b, &_| {
                    b.iter(|| {
                        rt.block_on(async {
                            let src = VecInputSource::new(make_inputs(num_inputs));
                            with_input_source(src, async {
                                let _ = std::hint::black_box(run_prove(&cfg)).await;
                            })
                            .await;
                        });
                    });
                });
            }
        }
    }

    group.finish();
}

criterion_group!(benches, prove_benchmarks);
criterion_main!(benches);
