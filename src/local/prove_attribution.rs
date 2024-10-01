use crate::local::ezkl::{
    compile_circuit, generate_circuit_settings, generate_proof, generate_witness, get_srs,
    setup_keys,
};
use crate::local::generate_model_identity;
use chrono::Local;
use serde_json::{json, Value};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use temp_dir::TempDir;

// Main function to handle the process
pub async fn prove_attribution(
    model_path: &Path,
    input_json: &Path,
    save_to_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory
    let tmp_dir = TempDir::new()?;
    let tmp_dir_path = tmp_dir.path();

    // Define paths for temporary files
    let settings_path = tmp_dir_path.join("settings.json");
    let srs_path = tmp_dir_path.join("kzg.srs");
    let compiled_model_path = tmp_dir_path.join("model.compiled");
    let pk_path = tmp_dir_path.join("pk.key");
    let vk_path = tmp_dir_path.join("vk.key");
    let witness_path = tmp_dir_path.join("witness.json");
    let proof_path = tmp_dir_path.join("proof.json");

    // Step 1: Generate circuit settings
    generate_circuit_settings(model_path, &settings_path)
        .await
        .map_err(|e| format!("Error generating model's settings: {}", e))?;

    // Step 2: Generate the SRS
    get_srs(&settings_path, &srs_path)
        .await
        .map_err(|e| format!("Error generating SRS: {}", e))?;

    // Step 3: Compile the circuit
    compile_circuit(model_path, &settings_path, &compiled_model_path)
        .await
        .map_err(|e| format!("Error compiling the model: {}", e))?;

    // Step 4: Setup proving and verification keys
    setup_keys(&compiled_model_path, &srs_path, &pk_path, &vk_path)
        .await
        .map_err(|e| format!("Error setting up model keys: {}", e))?;

    // Step 5: Generate the witness
    generate_witness(&compiled_model_path, input_json, &witness_path)
        .await
        .map_err(|e| format!("Error generating the witness: {}", e))?;

    // Step 6: Generate the proof
    generate_proof(
        &compiled_model_path,
        &pk_path,
        &witness_path,
        &proof_path,
        &srs_path,
    )
    .await
    .map_err(|e| format!("Error generating the proof: {}", e))?;

    // Generate the attribution certificate (JSON)
    let output_dir = save_to_path.unwrap_or_else(|| Path::new("."));
    let attribution_certificate_path = create_attribution_certificate(
        model_path,
        &proof_path,
        &settings_path,
        &vk_path,
        output_dir,
    )
    .map_err(|e| format!("Error creating the attribution certificate: {}", e))?;

    println!("======================================================");
    println!("   SUCCESS: The proof has been successfully generated!");
    println!("======================================================");
    println!(
        "   Attribution Certificate: {}",
        attribution_certificate_path.display()
    );
    println!("======================================================");

    Ok(())
}

// Function to generate the attribution certificate (JSON)
fn create_attribution_certificate(
    model_path: &Path,
    proof_path: &Path,
    settings_path: &Path,
    vk_path: &Path,
    output_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Get current date and time in format: "%Y-%m-%d %H:%M:%S"
    let current_date = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Read and parse the proof JSON file
    let proof_data = fs::read_to_string(proof_path)?;
    let proof_json: Value = serde_json::from_str(&proof_data)
        .map_err(|e| format!("Failed to parse proof JSON: {}", e))?;
    let settings_json: Value = serde_json::from_str(&fs::read_to_string(settings_path)?)
        .map_err(|e| format!("Failed to parse settings JSON: {}", e))?;
    let vk_bytes = fs::read(vk_path)?;
    let vk_str = hex::encode(&vk_bytes);

    let model_identity = generate_model_identity(Some(model_path), None, settings_path, vk_path)
        .map_err(|e| format!("Error generating model identity: {}", e))?;
    let model_identity_hash = model_identity
        .unique_indentifier()
        .map_err(|e| format!("Error hashing model identity: {}", e))?;

    // Create the attribution certificate as a JSON object
    let attribution_certificate = json!({
        "model_id": model_identity_hash,
        "generation_date": current_date,
        "proof": proof_json,
        "settings": settings_json,
        "vk": vk_str,
    });

    // Serialize the attribution certificate to a pretty JSON string
    let certificate_json = serde_json::to_string_pretty(&attribution_certificate).map_err(|e| {
        std::io::Error::new(
            ErrorKind::Other,
            format!("Failed to serialize attribution certificate: {}", e),
        )
    })?;

    // Write the attribution certificate JSON to the specified file

    let model_identity_hash = model_identity_hash[0..8].to_string();
    let attribution_certificate_path = output_dir.join(format!(
        "model_{}_attribution_certificate.json",
        model_identity_hash
    ));

    fs::write(&attribution_certificate_path, certificate_json)?;

    Ok(attribution_certificate_path)
}
