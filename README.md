# AI Passport

## Introduction

**AI Passport** is a system that generates unique cryptographic passports for machine learning models and verifies that
outputs are genuinely produced by those models. By leveraging cryptographic techniques and
the [EZKL](https://docs.ezkl.xyz/getting_started/) library, this system provides a robust framework for tracking and
authenticating AI-generated content.

This project enhances model verification by not only checking the consistency of the model's weights but also ensuring
the integrity of the verification keys and settings. This guarantees that the same model is being verified in the same
way, providing an additional layer of security and trust.

> **Note**: This is an experimental project and should be used with caution. It is not intended for production use
> without further testing and validation.

### Key Features

- **Model Passport Generation**: Creates a unique passport for your model, including hashes of critical components like
  weights, verification keys, and settings.
- **Content Attribution**: Generates cryptographic proofs that an output is derived from your model given specific input
  data.
- **Proof Verification**: Verifies the authenticity of the output by checking the proof and ensuring consistency across
  model components.
- **Remote Operations**: Supports remote interactions with AI services (e.g., Anthropic's API), allowing you to generate
  and verify proofs of remote AI model executions.

## Prerequisites

Before running this demo, ensure you have the following installed:

- **Rust**: Install Rust by following the instructions at [rust-lang.org](https://www.rust-lang.org/tools/install).
- **ONNX Model**: Have an ONNX model file available (e.g., `network.onnx`).
- **Input Data**: Prepare input data in JSON format compatible with your model (e.g., `input.json`).
- **API Access**: For remote operations, ensure you have access to the necessary APIs (e.g., Anthropic API) and have
  your API keys ready.

## Installation

### 1. Clone the Repository

```bash
git clone https://github.com/ElusAegis/ai-passport.git
cd ai-passport
```

### 2. Build the Project

```bash
cargo build --release
```

This command builds the `ai-passport` executable in the `target/release` directory.

## Running the Demo

The demo consists of several commands divided into local and remote operations:

### Local Operations

1. **create-passport**
2. **attribute-content**
3. **verify-attribution**

### Remote Operations

1. **anthropic-conversation**

---

## Local Operations

### Step 1: Create a Passport for the Model

Generate a unique passport for your ONNX model. The passport acts like a human passport, uniquely identifying the model
and including metadata.

**Command**:

```bash
cargo run --release -- local create-passport model/network.onnx --save-to ./model
```

**Explanation**:

- `local`: Specifies that the operation is for a local model.
- `create-passport`: The command to generate the model's passport.
- `model/network.onnx`: The path to your ONNX model file.
- `--save-to ./model`: *(Optional)* Specifies the directory to save the passport. Defaults to the current directory if
  omitted.

**What It Does**:

- **Generates a SHA256 hash of the model's weights**, acting as a unique identifier (`weight_hash`).
- **Generates SHA256 hashes of the verification key (`vk_hash`) and settings (`settings_hash`)**, ensuring consistency
  in the verification process.
- **Computes an overall `model_identity_hash`**, which is a SHA256 hash of the combined `weight_hash`, `vk_hash`, and
  `settings_hash`.
- **Collects metadata** such as generation date, model size, and optional fields like name, description, author, and
  source URL.
- **Creates a JSON passport file** containing the model's unique identity hash, metadata, and identity details.
- **Saves the passport file** as `model_network_<first_10_chars_of_hash>_passport.json` in the specified directory.

**Example Passport JSON**:

```json
{
  "model_identity_hash": "3a896386229e9068be5593b07ca3f2f972e7f93848fe5215c78fc19404cc6a64",
  "generation_date": "2024-09-26 13:29:28",
  "model_metadata": {
    "name": "network",
    "description": null,
    "author": null,
    "size_bytes": 13353,
    "source_url": null
  },
  "identity_details": {
    "vk_hash": "a1b8c7600613a968772bcdbaa6ef24e5af26b2cf3bf89989e4a5685116474b5d",
    "settings_hash": "8b0b9f15d69410fa2bc892c85f66fd391b9f081547855e93228e4066c92ec775",
    "weight_hash": "77de5c5fce890738da6770d51a8c2936ec8701c91d4849f88a8f71265f6fa664"
  }
}
```

### Step 2: Attribute Content to the Model

Generate a cryptographic proof that the output is derived from your model given specific input data.

**Command**:

```bash
cargo run --release -- local attribute-content model/network.onnx model/input.json --save-to ./model
```

**Explanation**:

- `local`: Specifies that the operation is for a local model.
- `attribute-content`: The command to attribute content to the model.
- `model/network.onnx`: The path to your ONNX model file.
- `model/input.json`: The path to the input data file.
- `--save-to ./model`: *(Optional)* Specifies the directory to save the output files.

**What It Does**:

- **Generates circuit settings and structured reference strings (SRS)** required for proof generation.
- **Compiles the model into a circuit** using the EZKL library.
- **Sets up proving and verification keys**.
- **Generates a witness** (intermediate computation results).
- **Produces a cryptographic proof** of the model's output given the input data.
- **Creates an attribution certificate** (JSON file) that includes the model's identity hash, generation date, proof,
  settings, and verification key.
- **Saves the attribution certificate** as `model_<first_10_chars_of_hash>_attribution_certificate.json` in the
  specified directory.

**Example Attribution Certificate JSON**:

```json
{
  "generation_date": "2024-09-26 13:29:49",
  "model_id": "3a896386229e9068be5593b07ca3f2f972e7f93848fe5215c78fc19404cc6a64",
  "proof": {
    "...": "..."
  },
  "settings": {
    "...": "..."
  },
  "vk": "..."
}
```

### Step 3: Verify the Attribution

Verify the cryptographic proof and ensure that the output is genuinely produced by your model.

**Command**:

```bash
cargo run --release -- local verify-attribution model/model_network_<model_hash>_passport.json model/model_<model_hash>_attribution_certificate.json
```

**Explanation**:

- `local`: Specifies that the operation is for a local model.
- `verify-attribution`: The command to verify the attribution proof.
- `model/model_network_<model_hash>_passport.json`: The path to the model's passport generated in Step 1.
- `model/model_<model_hash>_attribution_certificate.json`: The path to the attribution certificate generated in Step 2.

**What It Does**:

- **Reconstructs circuit settings and SRS** required for verification.
- **Compiles the model into a circuit**.
- **Sets up verification keys**.
- **Extracts the proof, settings, and verification key** from the attribution certificate.
- **Verifies the proof** using the EZKL library.
- **Computes the model's identity hash** and compares it with the `model_id` in the attribution certificate and the
  passport.
- **Ensures consistency** of the verification key and settings by comparing their hashes.
- **Outputs a success message** if verification passes.

**Note on `<model_hash>`**:

Replace `<model_hash>` with the first 10 characters of your model's identity hash, which is part of the filename of the
generated files.

---

## Remote Operations

With the addition of remote operations, you can now interact with AI models hosted remotely (e.g., via APIs) and
generate proofs of these interactions. The identity of the model is included in the proof, ensuring that the output is
genuinely produced by the specified model.

### Step 1: Interact with Anthropic's API and Generate a Proof

Engage in a conversation with an AI assistant (e.g., Anthropic's Claude) and generate a cryptographic proof of the
conversation.

**Command**:

```bash
cargo run --release -- remote anthropic-conversation
```

**Explanation**:

- `remote`: Specifies that the operation is for a remote model or service.
- `anthropic-conversation`: Initiates a conversation with Anthropic's AI assistant and generates a proof of the
  interaction.

**What It Does**:

- **Initiates a Conversation**: Starts an interactive session where you can send messages to the AI assistant and
  receive responses.
- **Generates a Cryptographic Proof**: After the conversation, the application generates a proof that the conversation
  occurred as recorded.
- **Saves the Proof**: The proof is saved to a file named `claude_conversation_proof.json`.

**Sample Output**:

```
🌟 Welcome to the Anthropic Prover CLI! 🌟
This application will interact with the Anthropic API to generate a cryptographic proof of your conversation.
💬 First, you will engage in a conversation with the assistant.
The assistant will respond to your messages in real time.
📝 When you're done, simply type 'exit' or press Enter without typing a message to end the conversation.
🔒 Once finished, a proof of the conversation will be generated.
💾 The proof will be saved as 'claude_conversation_proof.json' for your records.
✨ Let's get started! Begin by sending your first message.

💬 Your message:
> Hello, assistant!
```

After the conversation:

```
🔒 Generating a cryptographic proof of the conversation. Please wait...
✅ Proof successfully saved to `claude_conversation_proof.json`.
🔍 You can share this proof or inspect it at: https://explorer.tlsnotary.org/.
📂 Simply upload the proof, and anyone can verify its authenticity and inspect the details.
```

### Verifying the Proof

You can verify the execution of the model and inspect the conversation by uploading the proof file to
the [TLSNotary Explorer](https://explorer.tlsnotary.org/). This allows you and others to:

- **Verify Authenticity**: Confirm that the conversation took place exactly as recorded.
- **Inspect Details**: View the messages exchanged during the conversation.

---

## Future Work

- **Support for Additional Remote Models**: Plans to integrate support for other AI services and APIs.
- **Enhanced Metadata Input**: Allow users to input model metadata through CLI options or interactive prompts.
- **Additional Features**:
    - Support for different proof systems.
    - Integration with model repositories for automatic passport generation.
    - Enhanced error handling and user feedback.

---

Feel free to reach out if you have any questions or need further assistance!