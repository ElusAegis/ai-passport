//! Notary presets for automated benchmarking.

use ai_passport::{NotaryConfig, NotaryMode};
use dotenvy::var;

const KIB: usize = 1024;

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
    max_sent_bytes: 64 * KIB,
    max_recv_bytes: 64 * KIB,
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

/// All static notary presets.
const STATIC_NOTARY_PRESETS: &[&NotaryPreset] = &[&NOTARY_LOCAL, &NOTARY_PSE];

/// Get all available notary presets.
pub fn all_notary_presets() -> Vec<&'static NotaryPreset> {
    STATIC_NOTARY_PRESETS.to_vec()
}

/// Find a notary preset by name.
pub fn find_notary_preset(name: &str) -> Option<&'static NotaryPreset> {
    STATIC_NOTARY_PRESETS.iter().find(|p| p.name == name).copied()
}

/// Load notary presets based on environment configuration.
///
/// If `NOTARY_PRESETS` is set (comma-separated), use those presets only.
/// If none of the specified presets are valid, return empty list.
/// Otherwise, return all available presets.
pub fn load_notary_presets() -> Vec<&'static NotaryPreset> {
    if let Ok(preset_names) = var("NOTARY_PRESETS") {
        let names: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
        let mut presets = Vec::new();

        for name in &names {
            if let Some(preset) = find_notary_preset(name) {
                presets.push(preset);
            } else {
                tracing::warn!("Unknown notary preset '{}', skipping", name);
            }
        }

        if presets.is_empty() {
            let available: Vec<_> = STATIC_NOTARY_PRESETS.iter().map(|p| p.name).collect();
            tracing::warn!(
                "No valid notary presets found in NOTARY_PRESETS. Available: {}",
                available.join(", ")
            );
        } else {
            tracing::info!("Using {} notary preset(s) from NOTARY_PRESETS", presets.len());
        }

        return presets;
    }

    // Fall back to all presets
    all_notary_presets()
}