# AI Passport - Proofs of Autonomy

## Introduction

**AI Passport** is a framework for generating and verifying **cryptographic proofs of AI agent outputs**.
It provides a way to authenticate that a given conversation or tool call was genuinely produced by a specific model,
without having to trust the host running the agent.

The framework builds on the academic foundations introduced
in [VET: Verifiable Execution Traces](https://drive.google.com/file/d/1WxR3BzXjVkJdU46deZkrpNnmUapETQNm/view?usp=share_link)
and [Agent Proofs (ICML '25 workshop)](https://openreview.net/forum?id=3vC8POdixP).

Key properties:

- **Host-independent authentication**: proofs bind outputs to an *Agent Identity Document (AID)* rather than to a host.
- **Verifiable conversations**: any user can later verify that model responses came from the stated service and model ID.
- **Multiple proof strategies**: choose between TLS notarization, proxy attestation, or TEE-based proving depending on your trust model and performance requirements.

---

## Architecture

AI Passport supports multiple **prover types** for different use cases:

| Prover | Description | Best For | Overhead |
|--------|-------------|----------|----------|
| `direct` | Passthrough without proving | Development/testing | None |
| `proxy` | Attestation via proxy server | Low latency, trusted proxy | ~2-19% |
| `tls-single` | Single TLS session, proof at end | Short conversations | ~16-77% |
| `tls-per-message` | Fresh TLS per message | Long conversations, per-message proofs | ~16-77% (scales with rounds) |

### Supported API Providers

The CLI auto-detects provider-specific configurations based on the API domain:

- **Anthropic** (`api.anthropic.com`)
- **OpenAI** (via RedPill proxy at `api.red-pill.ai`)
- **Mistral** (`api.mistral.ai`)
- **Fireworks** (`api.fireworks.ai`)
- **Custom/Unknown** (OpenAI-compatible defaults)

---

## Quickstart

### Build

Clone and build all binaries:

```bash
git clone https://github.com/ElusAegis/ai-passport.git
cd ai-passport
cargo build --release
```

The binaries will appear under `target/release/`:
- `ai-passport` - Main CLI for proving and verifying
- `notary` - Local TLSNotary server
- `model-server` - Mock model server for testing
- `proxy-server` - Attestation proxy server

### Environment Setup

Create a `.env` file (see `.env.example`):

```bash
# Required: API credentials
MODEL_API_DOMAIN=api.anthropic.com
MODEL_API_PORT=443
MODEL_API_KEY=your-api-key-here

# Optional: Notary configuration (for TLS provers)
NOTARY_DOMAIN=notary.pse.dev
NOTARY_PORT=443
NOTARY_TYPE=remote

# Optional: Proxy configuration (for proxy prover)
PROXY_HOST=localhost
PROXY_PORT=8443
```

---

## CLI Usage

The CLI has two main commands: `prove` and `verify`.

### 1. Prove an Interaction

Start a session with a model, interact, and generate a cryptographic proof.

#### Using TLS Single-Shot Prover (default)

Best for short conversations where you want one proof at the end:

```bash
cargo run --release --bin ai-passport -- prove --prover tls-single
```

#### Using TLS Per-Message Prover

Best for longer conversations with per-message proofs:

```bash
cargo run --release --bin ai-passport -- prove --prover tls-per-message
```

#### Using Proxy Prover

Best for low-latency scenarios with a trusted attestation proxy:

```bash
cargo run --release --bin ai-passport -- prove --prover proxy --proxy-host proxy.example.com --proxy-port 8443
```

#### Using Direct Prover (No Proof)

For development and testing without cryptographic overhead:

```bash
cargo run --release --bin ai-passport -- prove --prover direct
```

#### Example Session

```
$ cargo run --release --bin ai-passport -- prove

◆ Welcome to the AI Passport CLI ◆
Create and verify cryptographic proofs of model conversations.

✔ Model Inference API · api.anthropic.com:443/v1/messages
✔ Model ID · claude-sonnet-4-5-20250929
✔ Configuration complete ✔

💬 Your message [↑ 3.9KB | ↓ 15.9KB]
(type 'exit' to end):
> Hello, what is 2+2?

🤖 Assistant's response:
(claude-sonnet-4-5-20250929) 2+2 equals 4.

💬 Your message [↑ 3.8KB | ↓ 15.8KB]
(type 'exit' to end):
> exit

✔ Proof successfully saved
📂 proofs/tls_claude-sonnet-4-5-20250929_single_shot_1733612345.json
```

### 2. Verify a Proof

Check that a saved proof corresponds to an authentic TLS-notarized session:

```bash
cargo run --release --bin ai-passport -- verify proofs/your_proof_file.json
```

Example output:

```
◆ Welcome to the AI Passport CLI ◆

✔ 📂 Proof file path · proofs/tls_claude-sonnet-4-5-20250929_single_shot_1733612345.json

🔑 Verifying presentation with key 0x037b48f1...
✔ Successfully verified bytes from a session with api.anthropic.com at 2025-12-07 13:31:04 UTC

📤 Messages sent:
POST /v1/messages HTTP/1.1
Host: api.anthropic.com
...

📥 Messages received:
HTTP/1.1 200 OK
{"id":"msg_01...","model":"claude-sonnet-4-5-20250929",...}
```

---

## CLI Options Reference

### Prove Command

```bash
ai-passport prove [OPTIONS]
```

| Option | Env Variable | Default | Description |
|--------|--------------|---------|-------------|
| `--prover` | `PROVER` | `tls-single` | Prover type: `direct`, `proxy`, `tls-single`, `tls-per-message` |
| `--model-id` | - | (interactive) | Model ID to use |
| `--env-file` | `APP_ENV_FILE` | `.env` | Path to environment file |

#### Notary Options (for TLS provers)

| Option | Env Variable | Default | Description |
|--------|--------------|---------|-------------|
| `--notary-type` | `NOTARY_TYPE` | `remote` | Notary mode: `remote`, `remote_non_tls`, `ephemeral` |
| `--notary-domain` | `NOTARY_DOMAIN` | `notary.pse.dev` | Notary server domain |
| `--notary-port` | `NOTARY_PORT` | `443` | Notary server port |
| `--notary-max-sent-bytes` | `NOTARY_MAX_SENT_BYTES` | `4096` | Max bytes to send |
| `--notary-max-recv-bytes` | `NOTARY_MAX_RECV_BYTES` | `16384` | Max bytes to receive |
| `--notary-network-optimization` | `NOTARY_NETWORK_OPTIMIZATION` | `latency` | Optimization: `latency` or `bandwidth` |

#### Proxy Options (for proxy prover)

| Option | Env Variable | Default | Description |
|--------|--------------|---------|-------------|
| `--proxy-host` | `PROXY_HOST` | `localhost` | Proxy server host |
| `--proxy-port` | `PROXY_PORT` | `8443` | Proxy server port |

---

## Components

### Attestation Proxy Server

The proxy server (`proxy-server`) provides a lightweight alternative to TLS notarization. It forwards requests to backend APIs while recording a transcript, which can be attested with a signature.

```bash
proxy-server --cert cert.pem --key key.pem --signing-key signing.pem --listen 0.0.0.0:8443
```

To get an attestation, clients send a request to `/__attest` after their conversation.

### Local Notary Server

For development or self-hosted deployments, run a local TLSNotary server:

```bash
cargo run --release --bin notary
```

### Mock Model Server

For testing without real API credentials:

```bash
cargo run --release --bin model-server
```

---

## Sample Agent

The repository includes a **sample agent** (`agent/`) demonstrating library integration into autonomous workflows.

The agent:
1. Fetches contextual data (Polymarket predictions + portfolio snapshot)
2. Builds a decision request
3. Sends it to a model API with proof generation
4. Produces both the decision output and a cryptographic proof transcript

### Run the sample agent

**Direct mode** (no attestation for data fetching):
```bash
cargo run --release --bin agent
```

**Attested mode** (data fetching via proxy with attestation):
```bash
# First, start the proxy server:
cargo run --release --bin proxy-server -- --cert cert.pem --key key.pem --signing-key signing.pem

# Then run the agent with attestation:
cargo run --release --bin agent -- --attested
```

In attested mode, the agent routes all external API calls (e.g., Polymarket data fetching) through the proxy server. This generates a cryptographic attestation proving the data was fetched from the actual API endpoint, ensuring the agent's decisions are based on authentic data.

Example output (attested mode):

```
Running in ATTESTED mode - fetching data via proxy
Connecting to proxy at localhost:8443
Fetched 3 markets via proxy
Data fetch attestation saved to: attestations/gamma-api_polymarket_com_1733612345.json
Polymarket context size: 1150 bytes
Portfolio context size: 1114 bytes
Decision request size: 2922 bytes
Success!
```

The attestation file can be verified to prove the agent received authentic data from the Polymarket API.

---

## Benchmarks

Performance benchmarks comparing different prover strategies are available in `benchmarks/`. Key findings:

| Prover Type | Round 1 Overhead | Notes |
|-------------|------------------|-------|
| Direct | 0% | Baseline (no proving) |
| Proxy | 2-19% | Minimal overhead |
| TEE-Proxy | 0.4-18% | Similar to proxy |
| TLS Notary | 16-77% | Scales with conversation length |

Run the benchmark analysis:

```bash
python3 cli/scripts/analyze_benchmarks.py --format report benchmarks/*.jsonl
```

---

## Development & Testing

### Local Development Setup

1. Start the mock model server:
   ```bash
   cargo run --bin model-server
   ```

2. Start a local notary (optional, for TLS provers):
   ```bash
   cargo run --bin notary
   ```

3. Start the proxy server (optional, for proxy prover):
   ```bash
   cargo run --bin proxy-server -- --cert cert.pem --key key.pem --signing-key signing.pem
   ```

4. Run the CLI:
   ```bash
   cargo run --bin ai-passport -- prove --prover direct
   ```

### Running Tests

```bash
cargo test
```

---

## Roadmap

- **TEE Integration**: Full support for Trusted Execution Environment attestations
- **Distributed Notaries**: MPC-backed notary pools for decentralized trust
- **AID Integration**: Export/import formal Agent Identity Documents
- **Extended Tools**: Support for non-LLM APIs and compositional traces
- **Proof Explorers**: Integration with TLSNotary Explorer for public inspection

---

## References

- Grigor et al., *Agent Proofs: Scalable and Practical Verification of AI Autonomy*. ICML Workshop 2025.
- Grigor et al., *VET Your Agent: Towards Host-Independent Autonomy via Verifiable Execution Traces*. In Review.
- [TLSNotary Documentation](https://tlsnotary.org/about)

---

## License

This project is licensed under the MIT License - see the LICENSE file for details.
