mod prove_attribution;
mod create_model_passport;
mod verify_attribution;
mod ezkl;

pub use create_model_passport::create_model_passport;
pub use prove_attribution::prove_attribution;
pub use verify_attribution::verify_attribution;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};
use std::fs;
use std::path::Path;

/// Helper function to generate the model's SHA256 hash
fn hash_file_content(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut hasher = Sha3_256::new();
    let file_data = fs::read(file_path)?;
    hasher.update(file_data);
    Ok(format!("{:x}", hasher.finalize()))
}

/// We need to remove the `timestamp` field from the settings file before hashing it
/// as the timestamp will change every time the settings file is generated.
fn hash_settings_file_content(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let settings_data = fs::read_to_string(file_path)?;
    let settings_json: serde_json::Value = serde_json::from_str(&settings_data)?;
    let settings_json = settings_json.as_object().ok_or("Error parsing settings JSON")?;
    let mut settings_json = settings_json.clone();
    settings_json.remove("timestamp");
    let settings_json = serde_json::to_string(&settings_json)?;
    let mut hasher = Sha3_256::new();
    hasher.update(settings_json);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Function to generate the model identity
fn generate_model_identity(model_path: Option<&Path>, weights_hash: Option<String>, settings_path: &Path, vk_path: &Path) -> Result<IdentityDetails, Box<dyn std::error::Error>> {
    let vk_hash = hash_file_content(&vk_path).map_err(|e| format!("Error generating VK hash: {}", e))?;
    let settings_hash = hash_settings_file_content(&settings_path).map_err(|e| format!("Error generating settings hash: {}", e))?;

    let weights_hash = if let Some(model_path) = model_path {
        hash_file_content(model_path).map_err(|e| format!("Error generating model hash: {}", e))?
    } else {
        weights_hash.expect("Weights hash or model path must be provided")
    };
    
    Ok(IdentityDetails {
        vk_hash,
        settings_hash,
        weight_hash: weights_hash.clone(),
    })
}

// TODO - check what is enough to guarantee uniqueness
#[derive(Serialize, Deserialize)]
struct IdentityDetails {
    pub(crate) vk_hash: String,
    pub(crate) settings_hash: String,
    pub(crate) weight_hash: String,
}

// Implement a method to generate the SHA256 hash for IdentityDetails
impl IdentityDetails {
    pub fn unique_indentifier(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Step 1: Serialize the struct to JSON (you can choose another format if needed)
        let serialized = serde_json::to_string(&self)?;

        // Step 2: Create a Sha256 hasher instance
        let mut hasher = Sha3_256::new();

        // Step 3: Feed the serialized data into the hasher
        hasher.update(serialized.as_bytes());

        // Step 4: Retrieve the hash result and convert it to a hexadecimal string
        let result = hasher.finalize();
        let hash_hex = format!("{:x}", result);

        // Step 5: Return the SHA256 hash as a string
        Ok(hash_hex)
    }
}