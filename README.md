## Introduction

This project demonstrates a system that generates unique cryptographic IDs for machine learning models and verifies that outputs are genuinely produced by those models. By leveraging the [EZKL](https://docs.ezkl.xyz/getting_started/) library, this system provides a robust framework for tracking and authenticating AI-generated content. The demo shows how to generate a unique ID for a model, produce a cryptographic proof that an output is derived from that model, and verify the authenticity of the output.

## Prerequisites

Before running this demo, ensure you have the following installed:

- **Docker**: [Install Docker](https://docs.docker.com/get-docker/)
- **EZKL Library**: Follow the installation instructions in the [EZKL documentation](https://docs.ezkl.xyz/installing/).

Alternatively, you can use Docker to run the entire demo without installing EZKL locally.

## Installation

### Option 1: Run with Docker

To use Docker, first build the Docker image:

```bash
docker build -t ezkl-tool .
```

### Option 2: Run Locally

If you prefer to run the demo locally, first install the EZKL library by following the [installation guide](https://docs.ezkl.xyz/installing/).

Then, clone this repository and navigate to the project directory.

## Running the Demo

This demo consists of three main scripts: `generate_id.sh`, `prove.sh`, and `verify.sh`.

### Step 1: Generate a Model ID

The first step is to generate a unique ID for your ONNX model. This ID is a SHA256 hash representing the model's identity.

Run the following command:

```bash
docker run -v $(pwd)/models:/app/models -v $(pwd)/data:/app/data ezkl-tool /app/scripts/generate_id.sh models/sample_model.onnx
```

This command generates a unique ID for the model and saves it in a file called `sample_model.onnx.sha256`. The output will also display the generated ID in the terminal.

### Step 2: Generate a Proof

Next, we generate a cryptographic proof that an output is derived from the model.

```bash
docker run -v $(pwd)/models:/app/models -v $(pwd)/data:/app/data ezkl-tool /app/scripts/prove.sh models/sample_model.onnx data/input.json
```

This command generates a proof and saves it to `proof.json`, along with an `origin_certificate.json` file containing the model's ID and the time of generation.

### Step 3: Verify the Proof

Finally, verify that the proof corresponds to the model.

```bash
docker run -v $(pwd)/models:/app/models -v $(pwd)/data:/app/data ezkl-tool /app/scripts/verify.sh models/sample_model.onnx proof.json origin_certificate.json
```

This command checks that the proof is valid and that the model's ID matches the one in the `origin_certificate.json`. If the verification is successful, the terminal will display a success message.

## Conclusion

This demo showcases a cryptographically secure method for ensuring the integrity and authenticity of machine learning models and their outputs. By generating unique IDs for models and providing verifiable proofs of output authenticity, we offer a framework that can be extended to broader applications, such as regulatory compliance, AI usage tracking, and more.

This system could serve as a foundation for securely managing and authenticating AI-generated content in various domains, ensuring that users can trust the origin and integrity of the data they receive.

--- 

This revised README provides a clear, structured, and actionable guide for users to understand the project, set up their environment, and run the demo effectively.