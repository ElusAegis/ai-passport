use clap::{Arg, Command};
use passport_for_ai::local;
use std::error::Error;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("ai-passport")
        .version("0.1")
        .about("AI Passport CLI")
        .subcommand(
            Command::new("create-passport")
                .about("Creates a unique passport for an ONNX model")
                .arg(
                    Arg::new("local")
                        .long("local")
                        .help("Indicates that this is for a local model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("remote"),
                )
                .arg(
                    Arg::new("remote")
                        .long("remote")
                        .help("Indicates that this is for a remote model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("local"),
                )
                .arg(
                    Arg::new("model_path")
                        .help("Path to the ONNX model file")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("save_to_path")
                        .long("save-to")
                        .help("Optional path to the directory where the passport will be saved")
                        .value_hint(clap::ValueHint::DirPath),
                ),
        )
        .subcommand(
            Command::new("attribute-content")
                .about("Attributes content to the model")
                .arg(
                    Arg::new("local")
                        .long("local")
                        .help("Indicates that this is for a local model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("remote"),
                )
                .arg(
                    Arg::new("remote")
                        .long("remote")
                        .help("Indicates that this is for a remote model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("local"),
                )
                .arg(
                    Arg::new("model_path")
                        .help("Path to the model to attribute the content to")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("content_path")
                        .help("Path to the content file to attribute")
                        .required(true)
                        .index(2),
                )
                .arg(
                    Arg::new("save_to_path")
                        .long("save-to")
                        .help("Optional path to the directory where the proof will be saved")
                        .value_hint(clap::ValueHint::DirPath),
                ),
        )
        .subcommand(
            Command::new("verify-attribution")
                .about("Verifies the cryptographic proof for a provided ONNX model")
                .arg(
                    Arg::new("local")
                        .long("local")
                        .help("Indicates that this is for a local model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("remote"),
                )
                .arg(
                    Arg::new("remote")
                        .long("remote")
                        .help("Indicates that this is for a remote model")
                        .action(clap::ArgAction::SetTrue)
                        .conflicts_with("local"),
                )
                .arg(
                    Arg::new("model_path")
                        .help("Path to the ONNX model file")
                        .required(true)
                        .index(1),
                )
                .arg(
                    Arg::new("attribution_certificate_path")
                        .help("Path to the attribution certificate file (attribution_certificate.json)")
                        .required(true)
                        .index(2),
                ),
        )
        .get_matches();

    if let Some(matches) = matches.subcommand_matches("create-passport") {
        let model_path = Path::new(matches.get_one::<String>("model_path").unwrap());
        let save_to_path = matches.get_one::<String>("save_to_path").map(Path::new);

        if matches.get_flag("local") {
            local::create_model_passport(model_path, save_to_path).map_err(|err| format!("Error generating model passport: {err}"))?;
        } else if matches.get_flag("remote") {
            eprintln!("Error: Remote models are not implemented yet.");
            std::process::exit(1);
        }
    }

    // Handle `attribute-content` command
    if let Some(matches) = matches.subcommand_matches("attribute-content") {
        let model_path = Path::new(matches.get_one::<String>("model_path").unwrap());
        let content_path = Path::new(matches.get_one::<String>("content_path").unwrap());
        let save_to_path = matches.get_one::<String>("save_to_path").map(Path::new);

        if matches.get_flag("local") {
            local::prove_attribution(model_path, content_path, save_to_path).await.map_err(|err| format!("Error attributing content to the model: {err}"))?;
        } else if matches.get_flag("remote") {
            eprintln!("Error: Remote models are not supported yet.");
            std::process::exit(1);
        }
    }

    // Handle `verify-attribution` command
    if let Some(matches) = matches.subcommand_matches("verify-attribution") {
        let model_path = Path::new(matches.get_one::<String>("model_path").unwrap());
        let attribution_certificate_path = Path::new(matches.get_one::<String>("attribution_certificate_path").unwrap());

        // Call the verify function
        local::verify_attribution(model_path, attribution_certificate_path).await.map_err(|err| format!("Error verifying attribution: {err}"))?;
    }

    Ok(())
}

