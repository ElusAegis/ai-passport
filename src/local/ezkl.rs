use ezkl::commands::Commands;
use ezkl::execute::run;
use ezkl::graph::Visibility;
use ezkl::{EZKLError, RunArgs};
use std::path::Path;

// Function to generate circuit settings
pub(super) async fn generate_circuit_settings(
    model_path: &Path,
    settings_path: &Path,
) -> Result<String, EZKLError> {
    let args = RunArgs {
        input_visibility: Visibility::Public,
        output_visibility: Visibility::Public,
        param_visibility: Visibility::Hashed {
            hash_is_public: true,
            outlets: vec![],
        },
        ..Default::default()
    };

    let gen_settings_command = Commands::GenSettings {
        model: Some(model_path.to_path_buf()),
        settings_path: Some(settings_path.to_path_buf()),
        args,
    };
    run(gen_settings_command).await
}

// Function to generate the structured reference string (SRS)
pub(super) async fn get_srs(settings_path: &Path, srs_path: &Path) -> Result<String, EZKLError> {
    let get_srs_command = Commands::GetSrs {
        srs_path: Some(srs_path.to_path_buf()),
        settings_path: Some(settings_path.to_path_buf()),
        logrows: None,
        commitment: None,
    };
    run(get_srs_command).await
}

// Function to compile the circuit
pub(super) async fn compile_circuit(
    model_path: &Path,
    settings_path: &Path,
    compiled_circuit_path: &Path,
) -> Result<String, EZKLError> {
    let compile_circuit_command = Commands::CompileCircuit {
        model: Some(model_path.to_path_buf()),
        settings_path: Some(settings_path.to_path_buf()),
        compiled_circuit: Some(compiled_circuit_path.to_path_buf()),
    };
    run(compile_circuit_command).await
}

// Function to setup proving and verification keys
pub(super) async fn setup_keys(
    compiled_circuit_path: &Path,
    srs_path: &Path,
    pk_path: &Path,
    vk_path: &Path,
) -> Result<String, EZKLError> {
    let setup_command = Commands::Setup {
        compiled_circuit: Some(compiled_circuit_path.to_path_buf()),
        srs_path: Some(srs_path.to_path_buf()),
        pk_path: Some(pk_path.to_path_buf()),
        vk_path: Some(vk_path.to_path_buf()),
        witness: None,
        disable_selector_compression: None,
    };
    run(setup_command).await
}

// Function to generate the witness
pub(super) async fn generate_witness(
    compiled_circuit_path: &Path,
    input_json: &Path,
    witness_path: &Path,
) -> Result<String, EZKLError> {
    let gen_witness_command = Commands::GenWitness {
        compiled_circuit: Some(compiled_circuit_path.to_path_buf()),
        data: Some(input_json.to_path_buf()),
        output: Some(witness_path.to_path_buf()),
        vk_path: None,
        srs_path: None,
    };
    run(gen_witness_command).await
}

// Function to generate the proof
pub(super) async fn generate_proof(
    compiled_circuit_path: &Path,
    pk_path: &Path,
    witness_path: &Path,
    proof_path: &Path,
    srs_path: &Path,
) -> Result<String, EZKLError> {
    let prove_command = Commands::Prove {
        compiled_circuit: Some(compiled_circuit_path.to_path_buf()),
        witness: Some(witness_path.to_path_buf()),
        pk_path: Some(pk_path.to_path_buf()),
        proof_path: Some(proof_path.to_path_buf()),
        srs_path: Some(srs_path.to_path_buf()),
        proof_type: Default::default(),
        check_mode: None,
    };
    run(prove_command).await
}

// Function to verify the proof
pub(super) async fn verify_proof(
    proof_path: &Path,
    settings_path: &Path,
    vk_path: &Path,
    srs_path: &Path,
) -> Result<String, EZKLError> {
    let verify_command = Commands::Verify {
        settings_path: Some(settings_path.to_path_buf()),
        proof_path: Some(proof_path.to_path_buf()),
        vk_path: Some(vk_path.to_path_buf()),
        srs_path: Some(srs_path.to_path_buf()),
        reduced_srs: Some(false),
    };
    run(verify_command).await
}
