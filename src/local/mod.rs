mod prove_attribution;
mod create_model_passport;
mod verify_attribution;
mod ezkl;

pub use create_model_passport::create_model_passport;
pub use prove_attribution::prove_attribution;
pub use verify_attribution::verify_attribution;

use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::path::Path;


// Helper function to generate the model's SHA256 hash
fn generate_model_id(model_path: &Path) -> Result<String, Box<dyn Error>> {
    let mut hasher = Sha256::new();
    let model_data = fs::read(model_path)?;
    hasher.update(model_data);
    Ok(format!("{:x}", hasher.finalize()))
}