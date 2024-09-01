#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Function to display usage instructions
function usage() {
  echo "Usage: $0 <path_to_model.onnx> <path_to_input.json>"
  echo "This script generates a cryptographic proof for the provided ONNX model."
  echo "You may optionally provide the paths for the model and input data."
  echo "If not provided, default paths 'network.onnx' and 'input.json' will be used."
  exit 1
}

# Function to handle errors and exit
function error_exit() {
  echo "======================================================"
  echo "   ERROR: $1"
  echo "   Script exited due to the above error."
  echo "======================================================"
  exit 1
}

# Check if the user provided the model and input data paths
MODEL_PATH=${1:-network.onnx}
INPUT_JSON=${2:-input.json}

# Check if the model file exists
if [ ! -f "$MODEL_PATH" ]; then
  error_exit "Model file not found at '$MODEL_PATH'. Please provide a valid file path."
fi

# Check if the input data file exists
if [ ! -f "$INPUT_JSON" ]; then
  error_exit "Input data file not found at '$INPUT_JSON'. Please provide a valid file path."
fi

# Create a temporary directory for generating artifacts
TMP_DIR=$(mktemp -d)
echo "Created temporary directory: $TMP_DIR"

# Step 1: Generate circuit settings
SETTINGS_PATH="$TMP_DIR/settings.json"
echo "Generating circuit settings..."
ezkl gen-settings -M "$MODEL_PATH" -O "$SETTINGS_PATH" > /dev/null 2>&1 || error_exit "Failed to generate circuit settings."
echo "Circuit settings generated and saved to $SETTINGS_PATH"

# Step 2: Get the structured reference string (SRS)
SRS_PATH="$TMP_DIR/kzg.srs"
echo "Generating the structured reference string (SRS)..."
ezkl get-srs --srs-path "$SRS_PATH" -S "$SETTINGS_PATH" > /dev/null 2>&1 || error_exit "Failed to generate the structured reference string (SRS)."
echo "SRS generated and saved to $SRS_PATH"

# Step 3: Compile the circuit
COMPILED_CIRCUIT_PATH="$TMP_DIR/model.compiled"
echo "Compiling the circuit..."
ezkl compile-circuit -M "$MODEL_PATH" -S "$SETTINGS_PATH" --compiled-circuit "$COMPILED_CIRCUIT_PATH" > /dev/null 2>&1 || error_exit "Failed to compile the circuit."
echo "Circuit compiled and saved to $COMPILED_CIRCUIT_PATH"

# Step 4: Setup keys (proving key and verification key)
VK_PATH="$TMP_DIR/vk.key"
PK_PATH="$TMP_DIR/pk.key"
echo "Setting up proving and verification keys..."
ezkl setup -M "$COMPILED_CIRCUIT_PATH" --srs-path="$SRS_PATH" --vk-path="$VK_PATH" --pk-path="$PK_PATH" > /dev/null 2>&1 || error_exit "Failed to setup keys."
echo "Verification key saved to $VK_PATH"
echo "Proving key saved to $PK_PATH"

# Step 5: Generate the witness
WITNESS_PATH="$TMP_DIR/witness.json"
echo "Generating the witness..."
ezkl gen-witness -M "$COMPILED_CIRCUIT_PATH" -D "$INPUT_JSON" -O "$WITNESS_PATH" > /dev/null 2>&1 || error_exit "Failed to generate the witness."
echo "Witness generated and saved to $WITNESS_PATH"

# Step 6: Generate the proof
PROOF_PATH="proof.json"
echo "Generating the proof..."
ezkl prove --proof-path "$PROOF_PATH" -M "$COMPILED_CIRCUIT_PATH" --pk-path "$PK_PATH" -W "$WITNESS_PATH" > /dev/null 2>&1 || error_exit "Failed to generate the proof."
echo "Proof generated and saved to $PROOF_PATH"

# Generate the SHA256 hash of the ONNX model
MODEL_HASH=$(sha256sum "$MODEL_PATH" | awk '{ print $1 }') || error_exit "Failed to generate the SHA256 hash of the model."

# Get the current date and time
CURRENT_DATE=$(date '+%Y-%m-%d %H:%M:%S')

# Create the origin certificate JSON
ORIGIN_CERTIFICATE_PATH="origin_certificate.json"
echo "{
  \"model_id\": \"$MODEL_HASH\",
  \"generation_date\": \"$CURRENT_DATE\"
}" > "$ORIGIN_CERTIFICATE_PATH" || error_exit "Failed to create the origin certificate."

# Final message
echo "======================================================"
echo "   SUCCESS: The proof has been successfully generated!"
echo "======================================================"
echo "   Proof File: $PROOF_PATH"
echo "   Model Unique ID (SHA256): $MODEL_HASH"
echo "   Origin Certificate: $ORIGIN_CERTIFICATE_PATH"
echo "   Temporary artifacts were generated in: $TMP_DIR"
echo "   (You may delete this directory if you no longer need the intermediate files.)"
echo "======================================================"