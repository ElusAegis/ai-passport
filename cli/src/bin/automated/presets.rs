//! Presets for automated benchmarking.
//!
//! Provides predefined notary and prover configurations for benchmarking.

use ai_passport::{
    AgentProver, DirectProver, NotaryConfig, NotaryMode, TlsPerMessageProver, TlsSingleShotProver,
};

const KIB: usize = 1024;

// ─────────────────────────────────────────────────────────────────────────────
// Notary Presets
// ─────────────────────────────────────────────────────────────────────────────

/// Notary preset configuration.
#[derive(Debug, Clone)]
pub struct NotaryPreset {
    /// Human-readable name for the preset.
    pub name: &'static str,
    /// Notary server domain.
    pub domain: &'static str,
    /// Notary server port.
    pub port: u16,
    /// Path prefix (version path), empty if none.
    pub path_prefix: &'static str,
    /// Connection mode.
    pub mode: NotaryMode,
    /// Maximum bytes that can be sent.
    pub max_sent_bytes: usize,
    /// Maximum bytes that can be received.
    pub max_recv_bytes: usize,
}

impl NotaryPreset {
    /// Build a NotaryConfig from this preset.
    pub fn to_notary_config(&self) -> NotaryConfig {
        NotaryConfig::builder()
            .domain(self.domain.to_string())
            .port(self.port)
            .path_prefix(self.path_prefix.to_string())
            .mode(self.mode)
            .max_total_sent(self.max_sent_bytes)
            .max_total_recv(self.max_recv_bytes)
            .max_decrypted_online(self.max_recv_bytes)
            .defer_decryption(false)
            .build()
            .expect("Failed to build NotaryConfig from preset")
    }
}

/// Local notary preset (localhost:7047, no TLS).
pub const NOTARY_LOCAL: NotaryPreset = NotaryPreset {
    name: "notary-local",
    domain: "localhost",
    port: 7047,
    path_prefix: "",
    mode: NotaryMode::RemoteNonTLS,
    max_sent_bytes: 16 * KIB,
    max_recv_bytes: 16 * KIB,
};

/// PSE notary preset (notary.pse.dev:443, TLS).
pub const NOTARY_PSE: NotaryPreset = NotaryPreset {
    name: "notary-pse",
    domain: "notary.pse.dev",
    port: 443,
    path_prefix: "v0.1.0-alpha.12",
    mode: NotaryMode::RemoteTLS,
    max_sent_bytes: 4 * KIB,
    max_recv_bytes: 16 * KIB,
};

// ─────────────────────────────────────────────────────────────────────────────
// Prover Presets
// ─────────────────────────────────────────────────────────────────────────────

/// Prover preset - a named function that builds an AgentProver.
pub struct ProverPreset {
    /// Human-readable name for the preset.
    pub name: &'static str,
    /// Function to build the prover (takes optional notary preset).
    build_fn: fn(&NotaryPreset) -> AgentProver,
}

impl ProverPreset {
    /// Build an AgentProver from this preset.
    pub fn build(&self, notary: &NotaryPreset) -> AgentProver {
        (self.build_fn)(&notary)
    }

    /// Whether this prover requires a notary.
    pub fn requires_notary(&self) -> bool {
        !matches!(self.name, "direct")
    }
}

/// Direct prover preset (no TLS notary, passthrough).
pub const PROVER_DIRECT: ProverPreset = ProverPreset {
    name: "direct",
    build_fn: |_| AgentProver::Direct(DirectProver::new()),
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

/// Get all available prover presets.
pub fn all_prover_presets() -> Vec<&'static ProverPreset> {
    vec![&PROVER_DIRECT, &PROVER_TLS_SINGLE, &PROVER_TLS_PER_MESSAGE]
}

/// Get all available notary presets.
pub fn all_notary_presets() -> Vec<&'static NotaryPreset> {
    vec![&NOTARY_LOCAL, &NOTARY_PSE]
}
