# AI Passport

## Introduction

This project demonstrates a system that generates unique cryptographic passports for machine learning models and
verifies that outputs are genuinely produced by those models. By leveraging
the [EZKL](https://docs.ezkl.xyz/getting_started/) library, this system provides a robust framework for tracking and
authenticating AI-generated content. The demo showcases how to:

- Generate a unique passport for a model.
- Produce a cryptographic proof that an output is derived from that model.
- Verify the authenticity of the output.

## Prerequisites

Before running this demo, ensure you have the following installed:

- **Rust**: Install Rust by following the instructions
  at [https://www.rust-lang.org/tools/install](https://www.rust-lang.org/tools/install).
- **EZKL Library**: Install the EZKL library by following the [installation guide](https://docs.ezkl.xyz/installing/).
- **ONNX Model**: Have an ONNX model file available (e.g., `network.onnx`).
- **Input Data**: Prepare input data in JSON format compatible with your model (e.g., `input.json`).

**Note**: Currently, this system supports only local models. Support for remote hosted models is under development.

## Installation

1. **Clone the Repository**:

   ```bash
   git clone https://github.com/yourusername/ai-passport.git
   cd ai-passport
   ```

2. **Build the Project**:

   ```bash
   cargo build --release
   ```

   This command builds the `ai-passport` executable in the `target/release` directory.

## Running the Demo

The demo consists of three main commands:

1. `create-passport`
2. `attribute-content`
3. `verify-attribution`

### Step 1: Create a Passport for the Model

Generate a unique passport for your ONNX model. The passport acts like a human passport, uniquely identifying the model
and including metadata.

**Command**:

```bash
cargo run --release -- create-passport --local model/network.onnx --save-to ./model
```

**Explanation**:

- `cargo run --release --`: Runs the compiled `ai-passport` binary in release mode.
- `create-passport`: The command to generate the model's passport.
- `--local`: Indicates that the model is stored locally.
- `model/network.onnx`: The path to your ONNX model file.
- `--save-to ./model`: *(Optional)* Specifies the directory to save the passport. Defaults to the current directory if
  omitted.

**What It Does**:

- Generates a SHA256 hash of the model's weights (the ONNX file), acting as a unique identifier (passport number).
- Collects metadata such as generation date and model size.
- Creates a JSON passport file containing the model's unique ID and metadata.
- Saves the passport file as `model_<first_10_chars_of_hash>_passport.json` in the specified directory.

*Example Passport JSON*:

```json
{
  "passport_number": "abc123def456...",
  "generation_date": "2023-09-25 15:45:30",
  "model_metadata": {
    "name": null,
    "description": null,
    "author": null,
    "size_bytes": 1234567,
    "source_url": null
  }
}
```

### Step 2: Attribute Content to the Model

Generate a cryptographic proof that the output is derived from your model given specific input data.

**Command**:

```bash
cargo run --release -- attribute-content --local model/input.json model/network.onnx --save-to ./model
```

**Explanation**:

- `attribute-content`: The command to attribute content to the model.
- `--local`: Indicates that both the model and content are local files.
- `model/input.json`: The path to the input data file.
- `model/network.onnx`: The path to your ONNX model file.
- `--save-to ./model`: *(Optional)* Specifies the directory to save the output files.

**What It Does**:

- Generates circuit settings and structured reference strings (SRS) required for proof generation.
- Compiles the model into a circuit.
- Sets up proving and verification keys.
- Generates a witness (intermediate computation results).
- Produces a cryptographic proof of the model's output given the input data.
- Creates an attribution certificate (JSON file) that includes the model's passport and the proof.
- Saves the attribution certificate as `model_<first_10_chars_of_hash>_attribution_certificate.json` in the specified
  directory.

*Example Attribution Certificate JSON*:

```json
{
  "model_id": "abc123def456...",
  "generation_date": "2023-09-25 16:00:00",
  "proof": {
    "proof_data": "...",
    "public_inputs": [
      "..."
    ]
  }
}
```

### Step 3: Verify the Attribution

Verify the cryptographic proof and ensure that the output is genuinely produced by your model.

**Command**:

```bash
cargo run --release -- verify-attribution model/network.onnx model/model_<model_hash>_attribution_certificate.json
```

**Explanation**:

- `verify-attribution`: The command to verify the attribution proof.
- `model/network.onnx`: The path to your ONNX model file.
- `model/model_<model_hash>_attribution_certificate.json`: The path to the attribution certificate generated in Step 2.

**What It Does**:

- Generates circuit settings and SRS required for verification.
- Compiles the model into a circuit.
- Sets up verification keys.
- Extracts the proof from the attribution certificate.
- Verifies the proof using the EZKL library.
- Computes the model's SHA256 hash and compares it with the `model_id` in the attribution certificate.
- Outputs a success message if verification passes.

**Note on `<model_hash>`**:

Replace `<model_hash>` with the first 10 characters of your model's SHA256 hash, which is part of the filename of the
generated attribution certificate.

## Future Work

- **Support for Remote Models**: Future updates will include the ability to handle models hosted remotely.
- **Enhanced Metadata Input**: Plans to allow users to input model metadata through CLI options or interactive prompts.
- **Additional Features**:
    - Support for different proof systems.
    - Integration with model repositories for automatic passport generation.

---

Feel free to reach out if you have any questions or need further assistance!