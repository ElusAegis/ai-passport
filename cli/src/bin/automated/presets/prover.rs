//! Prover presets for automated benchmarking.

use ai_passport::{AgentProver, DirectProver, ProxyConfig, ProxyProver, TlsPerMessageProver, TlsSingleShotProver};
use dotenvy::var;

use super::notary::NotaryPreset;

/// Prover preset - a named function that builds an AgentProver.
pub struct ProverPreset {
    /// Human-readable name for the preset.
    pub name: &'static str,
    /// Function to build the prover (takes notary preset).
    build_fn: fn(&NotaryPreset) -> AgentProver,
}

impl ProverPreset {
    /// Build an AgentProver from this preset.
    pub fn build(&self, notary: &NotaryPreset) -> AgentProver {
        (self.build_fn)(notary)
    }

    /// Whether this prover requires a notary.
    pub fn requires_notary(&self) -> bool {
        !matches!(self.name, "direct" | "proxy" | "proxy_tee")
    }
}

/// Direct prover preset (no TLS notary, passthrough).
pub const PROVER_DIRECT: ProverPreset = ProverPreset {
    name: "direct",
    build_fn: |_| AgentProver::Direct(DirectProver::new()),
};

/// Proxy prover preset (proxy.proof-of-autonomy.elusaegis.xyz:8443).
pub const PROVER_PROXY: ProverPreset = ProverPreset {
    name: "proxy",
    build_fn: |_| {
        let config = ProxyConfig {
            host: "proxy.proof-of-autonomy.elusaegis.xyz".to_string(),
            port: 8443,
        };
        AgentProver::Proxy(ProxyProver::new(config))
    },
};

/// Proxy TEE prover preset (proxy-tee.proof-of-autonomy.elusaegis.xyz:8443).
pub const PROVER_PROXY_TEE: ProverPreset = ProverPreset {
    name: "proxy_tee",
    build_fn: |_| {
        let config = ProxyConfig {
            host: "proxy-tee.proof-of-autonomy.elusaegis.xyz".to_string(),
            port: 8443,
        };
        AgentProver::Proxy(ProxyProver::new(config))
    },
};

/// TLS Single-Shot prover preset (single TLS session, proof at end).
pub const PROVER_TLS_SINGLE: ProverPreset = ProverPreset {
    name: "tls_single_shot",
    build_fn: |notary| {
        let config = notary.to_notary_config();
        AgentProver::TlsSingleShot(TlsSingleShotProver::new(config))
    },
};

/// TLS Per-Message prover preset (fresh TLS per message).
pub const PROVER_TLS_PER_MESSAGE: ProverPreset = ProverPreset {
    name: "tls_per_message",
    build_fn: |notary| {
        let config = notary.to_notary_config();
        AgentProver::TlsPerMessage(TlsPerMessageProver::new(config))
    },
};

/// All static prover presets.
const STATIC_PROVER_PRESETS: &[&ProverPreset] = &[
    &PROVER_DIRECT,
    &PROVER_PROXY,
    &PROVER_PROXY_TEE,
    &PROVER_TLS_SINGLE,
    &PROVER_TLS_PER_MESSAGE,
];

/// Get all available prover presets.
pub fn all_prover_presets() -> Vec<&'static ProverPreset> {
    STATIC_PROVER_PRESETS.to_vec()
}

/// Find a prover preset by name.
pub fn find_prover_preset(name: &str) -> Option<&'static ProverPreset> {
    STATIC_PROVER_PRESETS.iter().find(|p| p.name == name).copied()
}

/// Load prover presets based on environment configuration.
///
/// If `PROVER_PRESETS` is set (comma-separated), use those presets only.
/// If none of the specified presets are valid, return empty list.
/// Otherwise, return all available presets.
pub fn load_prover_presets() -> Vec<&'static ProverPreset> {
    if let Ok(preset_names) = var("PROVER_PRESETS") {
        let names: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
        let mut presets = Vec::new();

        for name in &names {
            if let Some(preset) = find_prover_preset(name) {
                presets.push(preset);
            } else {
                tracing::warn!("Unknown prover preset '{}', skipping", name);
            }
        }

        if presets.is_empty() {
            let available: Vec<_> = STATIC_PROVER_PRESETS.iter().map(|p| p.name).collect();
            tracing::warn!(
                "No valid prover presets found in PROVER_PRESETS. Available: {}",
                available.join(", ")
            );
        } else {
            tracing::info!("Using {} prover preset(s) from PROVER_PRESETS", presets.len());
        }

        return presets;
    }

    // Fall back to all presets
    all_prover_presets()
}