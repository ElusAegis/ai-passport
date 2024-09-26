use crate::local::ezkl::{compile_circuit, generate_circuit_settings, get_srs, setup_keys};
use crate::local::{generate_model_identity, IdentityDetails};
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::ErrorKind;
use std::path::Path;
use temp_dir::TempDir;

#[derive(Serialize, Deserialize, Default)]
struct ModelMetadata {
    name: String,
    description: Option<String>,
    author: Option<String>,
    size_bytes: u64,
    source_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ModelPassport {
    pub(crate) model_identity_hash: String,
    generation_date: String,
    model_metadata: ModelMetadata,
    identity_details: IdentityDetails,
}

impl ModelPassport {
    pub fn short_id(&self) -> String {
        format!("{}_{}", &self.model_metadata.name, &self.model_identity_hash[0..10])
    }
}

pub async fn create_model_passport(model_path: &Path, save_to_path: Option<&Path>) -> Result<(), Box<dyn Error>> {
    if !model_path.exists() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("Model file not found at '{}'. Please provide a valid file path.", model_path.display()),
        ).into());
    }

    if !is_valid_onnx_path(model_path) {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            format!("The provided file {} does not have a .onnx extension. Please provide a valid ONNX model file.", model_path.display()),
        ).into());
    }

    // Generate a unique model ID

    // Create a temporary directory
    let tmp_dir = TempDir::new()?;
    let tmp_dir_path = tmp_dir.path();

    // Define paths for temporary files
    let settings_path = tmp_dir_path.join("settings.json");
    let srs_path = tmp_dir_path.join("kzg.srs");
    let compiled_model_path = tmp_dir_path.join("model.compiled");
    let pk_path = tmp_dir_path.join("pk.key");
    let vk_path = tmp_dir_path.join("vk.key");

    generate_circuit_settings(model_path, &settings_path).await.map_err(|e| format!("Error generating model's settings: {}", e))?;
    get_srs(&settings_path, &srs_path).await.map_err(|e| format!("Error generating SRS: {}", e))?;
    compile_circuit(model_path, &settings_path, &compiled_model_path).await.map_err(|e| format!("Error compiling the model: {}", e))?;
    setup_keys(&compiled_model_path, &srs_path, &pk_path, &vk_path).await.map_err(|e| format!("Error setting up model keys: {}", e))?;


    let model_identity = generate_model_identity(Some(model_path), None, &settings_path, &vk_path).map_err(|e| format!("Error generating model identity: {}", e))?;

    // Get current date and time
    let current_date = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Get model size in bytes
    let metadata = std::fs::metadata(model_path)?;
    let model_size = metadata.len();

    // Create model metadata with default values
    let file_name = model_path.file_stem().and_then(|name| name.to_str()).map(|name| name.to_string()).expect("Error getting model name");
    let model_metadata = ModelMetadata {
        name: file_name,       // Default to file name of the model
        description: None,    // Default to None
        author: None,         // Default to None
        size_bytes: model_size,
        source_url: None,     // Default to None
    };

    let model_identity_hash = model_identity.unique_indentifier().map_err(|e| format!("Error hashing model identity: {}", e))?;

    // Create the model passport
    let model_passport = ModelPassport {
        model_identity_hash: model_identity_hash.clone(),
        identity_details: model_identity,
        generation_date: current_date,
        model_metadata,
    };

    let model_name = model_passport.short_id();

    // Serialize the model passport to JSON
    let passport_json = serde_json::to_string_pretty(&model_passport)?;

    println!("======================================================");
    println!("   SUCCESS: A unique passport has been generated for your model");
    println!("======================================================");
    println!("   Model Path: {}", model_path.display());
    println!("   Model Identity (SHA256 Hash):");
    println!("   {}", model_identity_hash);
    println!("======================================================");

    let output_file_name = format!("model_{}_passport.json", model_name);
    let output_dir = save_to_path.unwrap_or_else(|| Path::new("."));
    let output_file = output_dir.join(output_file_name);

    std::fs::write(&output_file, passport_json).expect("Unable to write passport to file");

    println!("Note: The model passport has been saved to '{}'.", output_file.display());

    Ok(())
}

fn is_valid_onnx_path(model_path: &Path) -> bool {
    model_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq("onnx"))
        .unwrap_or(false)
}