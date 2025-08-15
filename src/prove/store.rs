use anyhow::Context;
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub(super) fn store_proof_to_file<T: Serialize>(
    proof: &T,
    model_id: &str,
) -> anyhow::Result<PathBuf> {
    // Generate timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();

    // Create file path
    let sanitised_model_id = model_id.replace(" ", "_").replace("/", "_");
    let file_path = format!(
        "{}_{}_conversation_proof.json",
        sanitised_model_id, timestamp
    );
    let path_buf = PathBuf::from(&file_path);

    // Create and write to file
    let mut file = File::create(&path_buf).context("Failed to create proof file")?;

    let proof_content = serde_json::to_string_pretty(proof).context("Failed to serialize proof")?;

    file.write_all(proof_content.as_bytes())
        .context("Failed to write proof to file")?;

    Ok(path_buf)
}
