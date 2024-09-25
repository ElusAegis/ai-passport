use crate::local::ezkl::{compile_circuit, generate_circuit_settings, get_srs, setup_keys, verify_proof};
use crate::local::generate_model_id;
use serde_json::Value;
use std::error::Error;
use std::path::Path;
use temp_dir::TempDir;

// The verification function
pub async fn verify_attribution(
    model_path: &Path,
    attribution_certificate_path: &Path,
) -> Result<(), Box<dyn Error>> {
    // Check if the model, proof, and attribution certificate files exist
    if !model_path.exists() {
        return Err(format!("Model file not found at '{}'.", model_path.display()).into());
    }
    if !attribution_certificate_path.exists() {
        return Err(format!("Origin certificate file not found at '{}'.", attribution_certificate_path.display()).into());
    }

    // Step 1: Generate circuit settings (using existing command)
    let tmp_dir = TempDir::new()?;
    let settings_path = tmp_dir.path().join("settings.json");
    let srs_path = tmp_dir.path().join("kzg.srs");
    let compiled_model_path = tmp_dir.path().join("network.ezkl");
    let vk_path = tmp_dir.path().join("vk.key");
    let pk_path = tmp_dir.path().join("pk.key");
    let proof_path = tmp_dir.path().join("proof.json");

    // Call `gen-settings` command (reuse)
    generate_circuit_settings(model_path, &settings_path).await.map_err(|e| format!("Error generating model's settings: {}", e))?;

    // Call `get-srs` command (reuse)
    get_srs(&settings_path, &srs_path).await.map_err(|e| format!("Error generating SRS: {}", e))?;

    // Call `compile-circuit` command to compile the model to a circuit (reuse)
    compile_circuit(model_path, &settings_path, &compiled_model_path).await.map_err(|e| format!("Error compiling the model: {}", e))?;

    // Call `setup` command to generate verification key (reuse)
    setup_keys(&compiled_model_path, &srs_path, &pk_path, &vk_path).await.map_err(|e| format!("Error setting up model keys: {}", e))?;

    // Extract the proof from the attribution certificate
    let attribution_certificate_data = std::fs::read_to_string(attribution_certificate_path)?;
    let attribution_certificate_json: Value = serde_json::from_str(&attribution_certificate_data)?;

    let proof_json = attribution_certificate_json.get("proof")
        .ok_or("Proof not found in the attribution certificate.").map_err(|e| format!("Error extracting proof from the attribution certificate: {}", e))?;

    // Save the proof to a temporary file
    std::fs::write(&proof_path, serde_json::to_string(proof_json)?).map_err(|e| format!("Error saving proof to file: {}", e))?;

    // Step 3: Compile the circuit

    // Step 2: Verify the proof using `run`
    verify_proof(&proof_path, settings_path, srs_path, vk_path).await.map_err(|e| format!("Error verifying the proof: {}", e))?;

    // Step 3: Verify model ID with the attribution certificate
    let model_hash = generate_model_id(model_path).map_err(|e| format!("Error generating model ID: {}", e))?;

    let certificate_model_id = extract_model_id_from_certificate(attribution_certificate_path).map_err(|e| format!("Error extracting model ID from the attribution certificate: {}", e))?;

    if model_hash == certificate_model_id {
        println!("Model ID verification succeeded. The model's ID matches the attribution certificate.");
        println!("======================================================");
        println!("   SUCCESS: The proof has been successfully verified!");
        println!("======================================================");
    } else {
        eprintln!("======================================================");
        eprintln!("   WARNING: Model ID verification failed!");
        eprintln!("   The model ID does NOT match the attribution certificate.");
        eprintln!("======================================================");
        std::process::exit(1);
    }

    Ok(())
}

// Helper function to extract the model ID from the attribution certificate
fn extract_model_id_from_certificate(attribution_certificate_path: &Path) -> Result<String, Box<dyn Error>> {
    let certificate_data = std::fs::read_to_string(attribution_certificate_path)?;
    let certificate_model_id = certificate_data
        .lines()
        .find(|line| line.contains("\"model_id\""))
        .and_then(|line| line.split('"').nth(3))
        .ok_or("Failed to extract model ID from the attribution certificate.")?;
    Ok(certificate_model_id.to_string())
}