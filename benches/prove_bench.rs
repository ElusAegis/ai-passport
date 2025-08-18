// benches/prove_bench.rs
use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use rand::distr::Alphanumeric;
use rand::Rng;

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
    let redpill = models
        .iter()
        .find(|m| m.name == "redpill-remote")
        .unwrap()
        .clone();

    let notary_local = notaries
        .iter()
        .find(|n| n.name == "notary-local")
        .unwrap()
        .clone();
    let notary_pse = notaries
        .iter()
        .find(|n| n.name == "notary-pse")
        .unwrap()
        .clone();

    vec![(poa_local, notary_local), (redpill, notary_pse)]
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
    (0..n_msgs)
        .map(|_| Some(prompt_bytes(req_bytes)))
        .chain(std::iter::once(None))
        .collect()
}

/// Rebuild NotarisationConfig with updated per-message budgets.
fn rebuild_notarisation_with_sizes(
    base: &NotarisationConfig,
    new_req: usize,
    new_resp: usize,
) -> NotarisationConfig {
    base.create_builder()
        .max_single_request_size(new_req)
        .max_single_response_size(new_resp)
        .build()
        .expect("adjusted notarisation_config")
}

/// Estimate one-shot limits using your formula:
/// recv_limit = max_single_package_recv
/// sent_limit = max(sum_recv + sum_sent + max_single_sent, 2 * max_single_sent)
fn estimate_one_shot_limits(ncfg: &NotarisationConfig, n_msgs: usize) -> (usize, usize) {
    let s = ncfg.max_single_request_size;
    let r = ncfg.max_single_response_size;
    let sum_sent = s.saturating_mul(n_msgs);
    let sum_recv = r.saturating_mul(n_msgs);
    let sent_limit = std::cmp::max(sum_sent + sum_recv + s, 2 * s);
    let recv_limit = r;
    (sent_limit, recv_limit)
}

/// Try to shrink per-message sizes to fit notary caps (one-shot).
/// Returns (adjusted NotarisationConfig, req_bytes_for_generator, resp_cap_used).
fn fit_one_shot_to_caps(
    base: &NotarisationConfig,
    caps: NotaryCaps,
    n_msgs: usize,
) -> Option<(NotarisationConfig, usize, usize)> {
    // Cap the receive side first (can't exceed notary recv cap).
    let mut r = base.max_single_response_size.min(caps.max_recv_bytes);
    if r == 0 {
        return None;
    }

    // Bound for request size from two constraints:
    // 1) 2*s <= caps_sent  => s <= caps_sent/2
    // 2) s*(n+1) + r*n <= caps_sent => s <= (caps_sent - r*n)/(n+1)
    let cap2 = caps.max_sent_bytes / 2;
    let cap1 = if caps.max_sent_bytes > r.saturating_mul(n_msgs) {
        (caps.max_sent_bytes - r * n_msgs) / (n_msgs + 1)
    } else {
        0
    };

    let mut s = base.max_single_request_size.min(cap1).min(cap2);
    // Minimum viable sizes to keep the benchmark meaningful
    const MIN_REQ: usize = 128;
    const MIN_RESP: usize = 256;

    if s < MIN_REQ {
        // Try reducing response to free more headroom
        r = r.max(MIN_RESP);
        // shrink r down to notary cap if needed
        while s < MIN_REQ && r > MIN_RESP {
            r = std::cmp::max(MIN_RESP, r / 2);
            let cap1_try = if caps.max_sent_bytes > r.saturating_mul(n_msgs) {
                (caps.max_sent_bytes - r * n_msgs) / (n_msgs + 1)
            } else {
                0
            };
            s = base.max_single_request_size.min(cap1_try).min(cap2);
            if s >= MIN_REQ {
                break;
            }
        }
        if s < MIN_REQ {
            return None;
        }
    }

    let adj = rebuild_notarisation_with_sizes(base, s, r);
    // Final sanity check with estimate using your formula
    let (need_sent, need_recv) = estimate_one_shot_limits(&adj, n_msgs);
    if need_sent <= caps.max_sent_bytes && need_recv <= caps.max_recv_bytes {
        Some((adj, s, r))
    } else {
        None
    }
}

/// Try to shrink per-message sizes to fit notary caps (multi-round) using your `get_total_sent_recv_max`.
fn fit_multiround_to_caps(
    base: &NotarisationConfig,
    caps: NotaryCaps,
) -> Option<(NotarisationConfig, usize, usize)> {
    // Binary search a scale factor in (0,1] for both req/resp limits.
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    const MIN_REQ: usize = 128;
    const MIN_RESP: usize = 256;

    let orig_s = base.max_single_request_size as f64;
    let orig_r = base.max_single_response_size as f64;

    // If original already fits, keep it.
    {
        let (ts, tr) = get_total_sent_recv_max(base);
        if ts <= caps.max_sent_bytes && tr <= caps.max_recv_bytes {
            return Some((
                base.clone(),
                base.max_single_request_size,
                base.max_single_response_size,
            ));
        }
    }

    // 20 iterations is plenty
    let mut best: Option<(NotarisationConfig, usize, usize)> = None;
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        let s_try = (orig_s * mid).floor() as usize;
        let r_try = (orig_r * mid).floor() as usize;
        if s_try < MIN_REQ || r_try < MIN_RESP {
            break;
        }

        let adj = rebuild_notarisation_with_sizes(base, s_try, r_try);
        let (ts, tr) = get_total_sent_recv_max(&adj);

        if ts <= caps.max_sent_bytes && tr <= caps.max_recv_bytes {
            best = Some((adj, s_try, r_try));
            lo = mid; // try larger
        } else {
            hi = mid; // go smaller
        }
    }
    best
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
) -> ProveConfig {
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

    ProveConfig::builder()
        .model_config(model_config)
        .privacy_config(PrivacyConfig::default())
        .notarisation_config(notarisation_config)
        .build()
        .expect("prove_config")
}

// ───────────────────────────────────────────────────────────────────────────────
// Criterion benchmark
// ───────────────────────────────────────────────────────────────────────────────

pub fn prove_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("run_prove");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    // Input batches and aligned max-req constraints
    let input_cases: &[(usize, usize)] = &[(1, 1), (2, 2)];

    for (model, notary) in pairings() {
        for &net in &[NetOpt::Latency, NetOpt::Bandwidth] {
            for &mode in &[SessionMode::OneShot, SessionMode::MultiRound] {
                for &(num_inputs, max_req) in input_cases {
                    // Start with generous per-message budgets; they will be fitted to caps as needed.
                    let base_nc = NotarisationConfig::builder()
                        .notary_config(
                            NotaryConfig::builder()
                                .domain(notary.domain.to_string())
                                .port(notary.port)
                                .path_prefix(notary.version_path.to_string())
                                .mode(notary.mode)
                                .build()
                                .unwrap(),
                        )
                        .max_req_num_sent(max_req)
                        .max_single_request_size(256 * 1024)
                        .max_single_response_size(2 * 1024 * 1024)
                        .network_optimization(match net {
                            NetOpt::Latency => NetworkSetting::Latency,
                            NetOpt::Bandwidth => NetworkSetting::Bandwidth,
                        })
                        .mode(mode)
                        .build()
                        .unwrap();

                    // Capacity fit
                    let fit_result = match mode {
                        SessionMode::OneShot => {
                            fit_one_shot_to_caps(&base_nc, notary.caps, num_inputs)
                                .map(|(adj, req_bytes, _)| (adj, req_bytes))
                        }
                        SessionMode::MultiRound => fit_multiround_to_caps(&base_nc, notary.caps)
                            .map(|(adj, req_bytes, _)| (adj, req_bytes)),
                    };

                    let (adj_nc, req_bytes_for_gen) = match fit_result {
                        Some(x) => x,
                        None => {
                            eprintln!(
                                "SKIP: {}+{} {:?} {:?} inputs={} exceeds notary caps; unable to shrink further.",
                                model.name, notary.name, net, mode, num_inputs
                            );
                            continue;
                        }
                    };

                    // Build final ProveConfig using the adjusted budgets
                    let cfg = build_prove_config(
                        &model,
                        &notary,
                        net,
                        mode,
                        max_req,
                        adj_nc.max_single_request_size,
                        adj_nc.max_single_response_size,
                    );

                    let bid = BenchmarkId::new(
                        format!("{}+{}-{:?}-{:?}", model.name, notary.name, net, mode),
                        format!("inputs={}", num_inputs),
                    );
                    group.throughput(Throughput::Elements(num_inputs as u64));

                    // Single-threaded runtime to keep overhead predictable
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    group.bench_with_input(bid, &num_inputs, |b, &_| {
                        b.iter(|| {
                            rt.block_on(async {
                                let src =
                                    VecInputSource::new(make_inputs(num_inputs, req_bytes_for_gen));
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
    }

    group.finish();
}

criterion_group!(benches, prove_benchmarks);
criterion_main!(benches);
