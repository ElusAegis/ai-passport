use clap::{Arg, Command};
#[cfg(feature = "remote")]
use passport_for_ai::remote;
use std::error::Error;
#[cfg(feature = "local")]
use {passport_for_ai::local, std::path::Path};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new("ai-passport")
        .version("0.1")
        .about("AI Passport CLI")
        .subcommand(
            Command::new("local")
                .about("Operations for local models")
                .subcommand(
                    Command::new("create-passport")
                        .about("Creates a unique passport for a local ONNX model")
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
                        .about("Attributes content to the local model")
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
                        .about("Verifies the cryptographic proof for a local ONNX model")
                        .arg(
                            Arg::new("model_passport_path")
                                .help("Path to the JSON model passport")
                                .required(true)
                                .index(1),
                        )
                        .arg(
                            Arg::new("attribution_certificate_path")
                                .help("Path to the attribution certificate file (attribution_certificate.json)")
                                .required(true)
                                .index(2),
                        ),
                ),
        )
        .subcommand(
            Command::new("remote")
                .about("Operations for remote models")
                .subcommand(
                    Command::new("anthropic-conversation")
                        .about("Interact with the Anthropic API to generate an attribution proof of conversation"),
                )
                .subcommand(
                    Command::new("verify-attribution")
                        .about("Verifies the cryptographic proof for a remote model")
                        .arg(
                            Arg::new("proof_path")
                                .help("Path to the JSON proof file")
                                .required(true)
                                .index(1),
                        ),
                ),
        )
        .get_matches();

    #[allow(unused_variables)]
    // Handle `local` commands
    if let Some(local_matches) = matches.subcommand_matches("local") {
        #[cfg(feature = "local")]
        {
            if let Some(matches) = local_matches.subcommand_matches("create-passport") {
                let model_path = Path::new(matches.get_one::<String>("model_path").unwrap());
                let save_to_path = matches.get_one::<String>("save_to_path").map(Path::new);

                local::create_model_passport(model_path, save_to_path)
                    .await
                    .map_err(|err| format!("Error generating model passport: {}", err))?;
            } else if let Some(matches) = local_matches.subcommand_matches("attribute-content") {
                let model_path = Path::new(matches.get_one::<String>("model_path").unwrap());
                let content_path = Path::new(matches.get_one::<String>("content_path").unwrap());
                let save_to_path = matches.get_one::<String>("save_to_path").map(Path::new);

                local::prove_attribution(model_path, content_path, save_to_path)
                    .await
                    .map_err(|err| format!("Error attributing content to the model: {}", err))?;
            } else if let Some(matches) = local_matches.subcommand_matches("verify-attribution") {
                let model_passport_path =
                    Path::new(matches.get_one::<String>("model_passport_path").unwrap());
                let attribution_certificate_path = Path::new(
                    matches
                        .get_one::<String>("attribution_certificate_path")
                        .unwrap(),
                );

                local::verify_attribution(model_passport_path, attribution_certificate_path)
                    .await
                    .map_err(|err| format!("Error verifying attribution: {}", err))?;
            } else {
                eprintln!(
                    "Error: Invalid local subcommand. Use '--help' to see available commands."
                );
                std::process::exit(1);
            }
        }

        #[cfg(not(feature = "local"))]
        {
            eprintln!("Error: this binary was not compiled with the 'local' feature enabled. Hence, local operations are not supported.");
            std::process::exit(1);
        }
    }
    // Handle `remote` commands
    else if let Some(remote_matches) = matches.subcommand_matches("remote") {
        #[cfg(feature = "remote")]
        {
            if remote_matches
                .subcommand_matches("anthropic-conversation")
                .is_some()
            {
                remote::generate_conversation_attribution()
                    .await
                    .map_err(|err| format!("Error during conversation: {}", err))?;
            } else if let Some(matches) = remote_matches.subcommand_matches("verify-attribution") {
                let proof_path = matches.get_one::<String>("proof_path").unwrap();
                remote::verify_attribution(proof_path)
                    .map_err(|err| format!("Error verifying attribution: {}", err))?;
            } else {
                eprintln!("Error: The specified remote feature is not available yet. Currently, only 'anthropic-conversation' is supported.");
                std::process::exit(1);
            }
        }
        #[cfg(not(feature = "remote"))]
        {
            eprintln!("Error: this binary was not compiled with the 'remote' feature enabled. Hence, remote operations are not supported.");
            std::process::exit(1);
        }
    } else {
        eprintln!("Error: No valid subcommand provided. Use '--help' to see available commands.");
        std::process::exit(1);
    }

    Ok(())
}
