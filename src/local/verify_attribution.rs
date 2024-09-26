use crate::local::ezkl::verify_proof;
use crate::local::generate_model_identity;
use serde_json::Value;
use std::error::Error;
use std::path::Path;
use temp_dir::TempDir;

// The verification function
pub async fn verify_attribution(
    model_passport_path: &Path,
    attribution_certificate_path: &Path,
) -> Result<(), Box<dyn Error>> {
    // Check if the model, proof, and attribution certificate files exist
    if !model_passport_path.exists() {
        return Err(format!("Model file not found at '{}'.", model_passport_path.display()).into());
    }
    if !attribution_certificate_path.exists() {
        return Err(format!("Origin certificate file not found at '{}'.", attribution_certificate_path.display()).into());
    }

    // Step 1: Generate circuit settings (using existing command)
    let tmp_dir = TempDir::new()?;
    let settings_path = tmp_dir.path().join("settings.json");
    let vk_path = tmp_dir.path().join("vk.key");
    let proof_path = tmp_dir.path().join("proof.json");

    // Extract data from the attribution certificate
    let attribution_certificate_data = std::fs::read_to_string(attribution_certificate_path)?;
    let attribution_certificate_json: Value = serde_json::from_str(&attribution_certificate_data)?;

    let proof_json = attribution_certificate_json.get("proof")
        .ok_or("Proof not found in the attribution certificate.").map_err(|e| format!("Error extracting proof from the attribution certificate: {}", e))?;
    let settings_json = attribution_certificate_json.get("settings")
        .ok_or("Settings not found in the attribution certificate.").map_err(|e| format!("Error extracting settings from the attribution certificate: {}", e))?;
    let vk_json = attribution_certificate_json.get("vk")
        .ok_or("Verification key not found in the attribution certificate.").map_err(|e| format!("Error extracting VK from the attribution certificate: {}", e))?;
    let vk_bytes = hex::decode(vk_json.as_str().ok_or("Error decoding VK bytes")?)?;

    // Extract data from the model passport
    let model_passport_data = std::fs::read_to_string(model_passport_path)?;
    let model_passport_json: Value = serde_json::from_str(&model_passport_data)?;

    // Extract the identity_details.weight_hash from the model passport
    let weights_hash = model_passport_json.get("identity_details")
        .ok_or("Identity details not found in the model passport.").map_err(|e| format!("Error extracting identity details from the model passport: {}", e))?
        .get("weight_hash")
        .ok_or("Weight hash not found in the model passport.").map_err(|e| format!("Error extracting weight hash from the model passport: {}", e))?
        .as_str().ok_or("Error decoding weight hash")?.to_string();

    // Save the proof, vk, and settings to a temporary file
    std::fs::write(&proof_path, serde_json::to_string(proof_json)?).map_err(|e| format!("Error saving proof to file: {}", e))?;
    std::fs::write(&settings_path, serde_json::to_string(settings_json)?).map_err(|e| format!("Error saving settings to file: {}", e))?;
    std::fs::write(&vk_path, vk_bytes).map_err(|e| format!("Error saving VK to file: {}", e))?;

    // Step 2: Verify the proof using `run`
    verify_proof(&proof_path, &settings_path, &vk_path).await.map_err(|e| format!("Error verifying the proof: {}", e))?;

    // Step 3: Verify model ID with the attribution certificate
    println!("Weight hash: {}", weights_hash);
    let verified_model_identity = generate_model_identity(None, Some(weights_hash), &settings_path, &vk_path).map_err(|e| format!("Error generating model identity: {}", e))?;
    let verified_model_identity_hash = verified_model_identity.unique_indentifier().map_err(|e| format!("Error hashing model identity: {}", e))?;

    // Step 4: Extract the model ID from the model's passport
    let model_identity_in_passport = extract_model_id_from_passport(model_passport_path).map_err(|e| format!("Error extracting model ID from the attribution certificate: {}", e))?;

    // Step 5: Compare the model ID from the model's passport with the one in the attribution certificate
    if verified_model_identity_hash == model_identity_in_passport {
        println!("Model ID verification succeeded. The passport model indeed generated the stated output.");
        println!("======================================================");
        println!("   SUCCESS: The proof has been successfully verified!");
        println!("======================================================");
    } else {
        eprintln!("======================================================");
        eprintln!("   WARNING: Model ID verification failed!");
        eprintln!("   The passport model ID does NOT match the attribution certificate.");
        eprintln!("   The model passport ID: {}", model_identity_in_passport);
        eprintln!("   The proof model ID: {}", verified_model_identity_hash);
        eprintln!("======================================================");
        std::process::exit(1);
    }

    Ok(())
}

// Helper function to extract the model ID from the attribution certificate
fn extract_model_id_from_passport(model_passport_path: &Path) -> Result<String, Box<dyn Error>> {
    let attribution_certificate_data = std::fs::read_to_string(model_passport_path)?;
    let attribution_certificate_json: Value = serde_json::from_str(&attribution_certificate_data)?;

    let model_id = attribution_certificate_json.get("model_identity_hash")
        .ok_or("Model ID key `model_identity_hash` not found in the model passport.").map_err(|e| format!("Error extracting model ID from the model passport: {}", e))?;
    Ok(model_id.as_str().ok_or("Error decoding model ID")?.to_string())
}