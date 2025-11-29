use dialoguer::console::style;
use std::path::PathBuf;
use tracing::info;

pub(crate) fn display_proofs(stored_proofs: &[PathBuf]) {
    // Display results
    if !stored_proofs.is_empty() {
        info!(target: "plain",
            "\n{} {}",
            style("✔").green(),
            style("All proofs successfully saved").bold(),
        );

        for (i, proof) in stored_proofs.iter().enumerate() {
            info!(target: "plain", "{} Assistant message {} → {}", style("📂").dim(), i + 1, proof.display());
        }

        info!(target: "plain",
            "\n{} {}",
            style("🔍").yellow(),
            style("You can verify these proofs anytime with the CLI: `verify <proof_file>`").dim()
        );
    } else {
        info!(target: "plain", "No proofs were generated during this session.");
    }
}
