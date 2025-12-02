//! Model presets for automated benchmarking.

use dotenvy::var;

/// Model preset configuration for API endpoints.
#[derive(Debug, Clone)]
pub struct ModelPreset {
    /// Human-readable name for the preset.
    pub name: String,
    /// Environment variable name for the API key.
    pub api_key_env: String,
    /// Model API domain.
    pub domain: String,
    /// Model API port.
    pub port: u16,
    /// Model ID to use.
    pub model_id: String,
}

impl ModelPreset {
    /// Load the API key from the environment variable.
    pub fn load_api_key(&self) -> Result<String, std::env::VarError> {
        std::env::var(&self.api_key_env)
    }

    /// Build an ApiProvider from this preset.
    pub fn build_api_provider(&self) -> ai_passport::ApiProvider {
        let api_key = self
            .load_api_key()
            .expect("Failed to load API key from environment");
        ai_passport::ApiProvider::builder()
            .domain(self.domain.clone())
            .port(self.port)
            .api_key(api_key)
            .build()
            .expect("Failed to build ApiProvider")
    }
}

/// Static model preset definition (for const definitions).
struct StaticModelPreset {
    name: &'static str,
    api_key_env: &'static str,
    domain: &'static str,
    port: u16,
    model_id: &'static str,
}

impl StaticModelPreset {
    const fn new(
        name: &'static str,
        api_key_env: &'static str,
        domain: &'static str,
        port: u16,
        model_id: &'static str,
    ) -> Self {
        Self {
            name,
            api_key_env,
            domain,
            port,
            model_id,
        }
    }

    fn to_owned(&self) -> ModelPreset {
        ModelPreset {
            name: self.name.to_string(),
            api_key_env: self.api_key_env.to_string(),
            domain: self.domain.to_string(),
            port: self.port,
            model_id: self.model_id.to_string(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Static Preset Definitions
// ─────────────────────────────────────────────────────────────────────────────

// Custom (proof-of-autonomy) presets
const MODEL_CUSTOM_INSTANT: StaticModelPreset = StaticModelPreset::new(
    "custom-instant",
    "CUSTOM_API_KEY",
    "api.proof-of-autonomy.elusaegis.xyz",
    3000,
    "instant",
);

const MODEL_CUSTOM_DEMO_GPT4O_MINI: StaticModelPreset = StaticModelPreset::new(
    "custom-demo-gpt-4o-mini",
    "CUSTOM_API_KEY",
    "api.proof-of-autonomy.elusaegis.xyz",
    3000,
    "demo-gpt-4o-mini",
);

// Anthropic presets
const MODEL_ANTHROPIC_HAIKU: StaticModelPreset = StaticModelPreset::new(
    "anthropic-haiku",
    "ANTHROPIC_API_KEY",
    "api.anthropic.com",
    443,
    "claude-haiku-4-5-20251001",
);

// Phala (Red Pill) presets
const MODEL_PHALA_HAIKU: StaticModelPreset = StaticModelPreset::new(
    "phala-haiku",
    "REDPILL_API_KEY",
    "api.red-pill.ai",
    443,
    "claude-haiku-4-5-20251001",
);

/// All static model presets.
const STATIC_MODEL_PRESETS: &[StaticModelPreset] = &[
    MODEL_CUSTOM_INSTANT,
    MODEL_CUSTOM_DEMO_GPT4O_MINI,
    MODEL_ANTHROPIC_HAIKU,
    MODEL_PHALA_HAIKU,
];

// ─────────────────────────────────────────────────────────────────────────────
// Preset Loading Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Get all available model presets as owned values.
pub fn all_model_presets() -> Vec<ModelPreset> {
    STATIC_MODEL_PRESETS.iter().map(|p| p.to_owned()).collect()
}

/// Find a model preset by name.
pub fn find_model_preset(name: &str) -> Option<ModelPreset> {
    STATIC_MODEL_PRESETS
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.to_owned())
}

/// Try to load a custom model preset from environment variables.
/// Returns None if the required env vars are not set.
fn try_load_custom_preset_from_env() -> Option<ModelPreset> {
    let domain = var("MODEL_API_DOMAIN").ok()?;
    let model_id = var("MODEL_ID").ok()?;
    let port = var("MODEL_API_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(443);
    let api_key_env = "MODEL_API_KEY".to_string();

    Some(ModelPreset {
        name: format!("custom-{}", model_id),
        api_key_env,
        domain,
        port,
        model_id,
    })
}

/// Load model presets based on environment configuration.
///
/// Priority:
/// 1. If `MODEL_PRESETS` is set (comma-separated), use those presets only.
///    If none of the specified presets are valid, return empty list.
/// 2. If `MODEL_API_DOMAIN` and `MODEL_ID` are set, use a custom preset
/// 3. Otherwise, return all available presets
pub fn load_model_presets() -> Vec<ModelPreset> {
    // Check for explicit preset selection (comma-separated list)
    if let Ok(preset_names) = var("MODEL_PRESETS") {
        let names: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
        let mut presets = Vec::new();

        for name in &names {
            if let Some(preset) = find_model_preset(name) {
                presets.push(preset);
            } else {
                tracing::warn!("Unknown model preset '{}', skipping", name);
            }
        }

        if presets.is_empty() {
            let available: Vec<_> = STATIC_MODEL_PRESETS.iter().map(|p| p.name).collect();
            tracing::warn!(
                "No valid model presets found in MODEL_PRESETS. Available: {}",
                available.join(", ")
            );
        } else {
            tracing::info!("Using {} model preset(s) from MODEL_PRESETS", presets.len());
        }

        return presets;
    }

    // Try to load custom preset from env vars
    if let Some(custom) = try_load_custom_preset_from_env() {
        tracing::info!("Using custom model preset from env vars: {}", custom.name);
        return vec![custom];
    }

    // Fall back to all presets
    tracing::info!("No MODEL_PRESETS or MODEL_API_DOMAIN set, using all model presets");
    all_model_presets()
}
