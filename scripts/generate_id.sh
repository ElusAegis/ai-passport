#!/bin/bash

# Script to generate a unique ID for an ONNX model by computing its SHA256 hash.

# Function to display usage instructions
function usage() {
  echo "Usage: $0 <path_to_model.onnx>"
  echo "This script generates a unique ID for the provided ONNX model by computing its SHA256 hash."
  echo "Make sure to provide a valid ONNX model file."
  exit 1
}

# Check if the user provided an argument
if [ -z "$1" ]; then
  echo "Error: No file provided."
  usage
fi

MODEL_PATH=$1

# Check if the file exists
if [ ! -f "$MODEL_PATH" ]; then
  echo "Error: File not found at '$MODEL_PATH'. Please provide a valid file path."
  usage
fi

# Check if the file has a .onnx extension
if [[ "$MODEL_PATH" != *.onnx ]]; then
  echo "Error: The provided file does not have a .onnx extension. Please provide a valid ONNX model file."
  usage
fi

# Generate SHA256 hash of the ONNX model
MODEL_HASH=$(sha256sum "$MODEL_PATH" | awk '{ print $1 }')

# Output the result with a user-friendly message
echo "======================================================"
echo "   SUCCESS: A unique ID has been generated for your model"
echo "======================================================"
echo "   Model Path: $MODEL_PATH"
echo "   Model Unique ID (SHA256):"
echo "   $MODEL_HASH"
echo "======================================================"

# Save the hash to a file
echo $MODEL_HASH > "${MODEL_PATH}.sha256"

# Confirmation message for file creation
echo "Note: The unique ID has been saved to '${MODEL_PATH}.sha256'."