use anyhow::Result;
use dialoguer::console::style;
use dialoguer::{theme::ColorfulTheme, Input};
use std::path::{Path, PathBuf};

pub(crate) fn select_proof_path() -> Result<PathBuf> {
    // Prompt with validation: path must exist and be a regular file
    let raw: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "{} {}",
            style("📂").dim(),
            style("Proof file path").bold()
        ))
        .validate_with(|s: &String| -> std::result::Result<(), String> {
            let p = Path::new(s);
            if !p.exists() {
                return Err("Path does not exist".into());
            }
            let meta = std::fs::metadata(p).map_err(|e| format!("Cannot access path: {e}"))?;
            if !meta.is_file() {
                return Err("Path is not a regular file".into());
            }
            Ok(())
        })
        .interact_text()?;

    // Canonicalize if possible; fall back to the raw path
    let path = std::fs::canonicalize(&raw).unwrap_or_else(|_| PathBuf::from(&raw));

    Ok(path)
}
