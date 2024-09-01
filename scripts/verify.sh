#!/bin/bash

# Exit immediately if a command exits with a non-zero status
set -e

# Function to display usage instructions
function usage() {
  echo "Usage: $0 <path_to_model.onnx> <path_to_proof.json> <path_to_origin_certificate.json>"
  echo "This script verifies a cryptographic proof for the provided ONNX model."
  echo "You must provide the paths for the model, proof file, and origin certificate."
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

# Check if the user provided the model, proof, and origin certificate paths
MODEL_PATH=$1
PROOF_PATH=${2:-proof.json}
ORIGIN_CERTIFICATE_PATH=${3:-origin_certificate.json}

# Check if the model file exists
if [ ! -f "$MODEL_PATH" ]; then
  error_exit "Model file not found at '$MODEL_PATH'. Please provide a valid file path."
fi

# Check if the proof file exists
if [ ! -f "$PROOF_PATH" ]; then
  error_exit "Proof file not found at '$PROOF_PATH'. Please provide a valid file path."
fi

# Check if the origin certificate file exists
if [ ! -f "$ORIGIN_CERTIFICATE_PATH" ]; then
  error_exit "Origin certificate file not found at '$ORIGIN_CERTIFICATE_PATH'. Please provide a valid file path."
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

# Step 5: Verify the proof
echo "Verifying the proof..."
ezkl verify --proof-path "$PROOF_PATH" -S "$SETTINGS_PATH" --srs-path="$SRS_PATH" --vk-path "$VK_PATH" > /dev/null 2>&1
if [ $? -ne 0 ]; then
  error_exit "Proof verification failed!"
else
  echo "Proof verification succeeded."
fi
# Step 6: Validate the model's ID with the origin certificate
echo "Validating the model's ID with the origin certificate..."

# Generate the SHA256 hash of the ONNX model
MODEL_HASH=$(sha256sum "$MODEL_PATH" | awk '{ print $1 }') || error_exit "Failed to generate the SHA256 hash of the model."

# Extract the ID from the origin certificate
CERTIFICATE_MODEL_ID=$(grep '"model_id"' "$ORIGIN_CERTIFICATE_PATH" | cut -d '"' -f 4) || error_exit "Failed to extract model ID from the origin certificate."

# Compare the model's ID with the certificate's ID
if [ "$MODEL_HASH" = "$CERTIFICATE_MODEL_ID" ]; then
  echo "Model ID verification succeeded. The model's ID matches the origin certificate."
  echo "======================================================"
  echo "   SUCCESS: The proof has been successfully verified!"
  echo "   The model ID matches the origin certificate."
  echo "======================================================"
else
  echo "======================================================"
  echo "   WARNING: Model ID verification failed!"
  echo "   The model ID does NOT match the origin certificate."
  echo "======================================================"
  exit 1
fi

# Final message
echo "======================================================"
echo "   SUCCESS: The proof and model ID have been successfully verified!"
echo "======================================================"
echo "   Proof File: $PROOF_PATH"
echo "   Model Unique ID (SHA256): $MODEL_HASH"
echo "   Origin Certificate: $ORIGIN_CERTIFICATE_PATH"
echo "   Temporary artifacts were generated in: $TMP_DIR"
echo "   (You may delete this directory if you no longer need the intermediate files.)"
echo "======================================================"