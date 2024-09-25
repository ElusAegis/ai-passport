use crate::local::generate_model_id;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct ModelMetadata {
    name: Option<String>,
    description: Option<String>,
    author: Option<String>,
    size_bytes: u64,
    source_url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ModelPassport {
    passport_number: String,
    generation_date: String,
    model_metadata: ModelMetadata,
}

pub fn create_model_passport(model_path: &Path, save_to_path: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
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

    let model_hash = generate_model_id(model_path)
        .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, format!("Error generating model hash: {}", e)))?;

    // Get current date and time
    let current_date = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Get model size in bytes
    let metadata = std::fs::metadata(model_path)?;
    let model_size = metadata.len();

    // Create model metadata with default values
    let model_metadata = ModelMetadata {
        name: None,           // Default to None
        description: None,    // Default to None
        author: None,         // Default to None
        size_bytes: model_size,
        source_url: None,     // Default to None
    };

    // Create the model passport
    let model_passport = ModelPassport {
        passport_number: model_hash.clone(),
        generation_date: current_date,
        model_metadata,
    };

    // Serialize the model passport to JSON
    let passport_json = serde_json::to_string_pretty(&model_passport)?;

    println!("======================================================");
    println!("   SUCCESS: A unique passport has been generated for your model");
    println!("======================================================");
    println!("   Model Path: {}", model_path.display());
    println!("   Passport Number (Model SHA256 Hash):");
    println!("   {}", model_hash);
    println!("======================================================");

    let output_file_name = format!("model_{}_passport.json", &model_hash[0..10]);
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