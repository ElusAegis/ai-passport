# VET Your Agent (Proofs of Autonomy)

## Introduction

**VET Your Agent** is a framework for generating and verifying **cryptographic proofs of AI agent outputs**.  
It provides a way to authenticate that a given conversation or tool call was genuinely produced by a specific agent
configuration, without having to trust the host running the agent.

The framework builds on the academic foundations introduced
in [VET: Verifiable Execution Traces (In Review)](https://drive.google.com/file/d/1WxR3BzXjVkJdU46deZkrpNnmUapETQNm/view?usp=share_link)
and [Agent Proofs (ICML ’25 workshop)](https://openreview.net/forum?id=3vC8POdixP).  
In practice, VET instantiates proofs using **TLSNotary** (“Web Proofs”) — cryptographically notarized TLS transcripts —
which allow you to prove that an HTTPS interaction with a model API happened as claimed.

Key properties:

- **Host-independent authentication**: proofs bind outputs to an *Agent Identity Document (AID)* rather than to a host.
- **Verifiable conversations**: any user can later verify that model responses came from the stated service and model
  ID.
- **Composable**: works with multiple proof systems in theory, but the current implementation is focused on **TLSNotary
  **.

---

## Quickstart

### Build

Clone and build all binaries:

```
git clone https://github.com/ElusAegis/ai-passport.git
cd ai-passport
cargo build --release
```

The relevant binaries (`cli`, `notary`, `model-server`) will appear under `target/release/`.

### Environment Setup

Configure the runtime via environment variables. See `.env.example` for a template:

- `MODEL_API_DOMAIN`, `MODEL_API_PORT`, `MODEL_API_KEY` – inference server details.
- `NOTARY_DOMAIN`, `NOTARY_PORT`, `NOTARY_TYPE` – notary settings (ephemeral, remote, or public).
- `SERVER_TLS_CERT`, `SERVER_TLS_KEY` – TLS certs if running a dummy local server.

You can either spin up **dummy local services** (model-server + notary) for testing, or point the CLI at a **remote
TLSNotary** and a real model API (e.g. Anthropic, OpenAI).

---

## CLI Usage

The CLI has two main flows: `prove` and `verify`.

### 1. Prove an Interaction

Start a session with a model, interact, and generate a cryptographic proof.

```
cargo run --bin cli -- prove --notary-mode ephemeral
```

Example run:

```
◆ Welcome to the Proofs-of-Autonomy CLI ◆
Create and verify cryptographic proofs of model conversations.

✔ API key set through ENV
? Model to interact with (type to filter) ›  
❯ demo-gpt-4o-mini
  demo-gpt-3.5-turbo
  Enter model ID manually...

✔ Model Inference API · api.proof-of-autonomy.elusaegis.xyz:3000
✔ Notary API · localhost:7047/ (mode: RemoteNonTLS)
✔ Protocol Session Mode · single
✔ Max Requests · 3

💬 Your message
(type 'exit' to end): 
> Test Message

🤖 Assistant's response:
(demo-gpt-4o-mini) You said: "Test Message" — fixed reply.

> exit

✔ Proof successfully saved
📂 proofs/demo-gpt-4o-mini_single_setup_interaction_proof_1756474286.json
```

---

### 2. Verify a Proof

Check that a saved proof corresponds to an authentic TLS-notarized session with the target model API.

```
cargo run --bin cli -- verify proofs/demo-gpt-4o-mini_single_setup_interaction_proof_1756474286.json
```

Example output:

```
◆ Welcome to the Proofs-of-Autonomy CLI ◆

✔ 📂 Proof file path · proofs/demo-gpt-4o-mini_single_setup_interaction_proof_1756474286.json

🔑 Verifying presentation with key 0x037b48f1...
✔ Successfully verified bytes from a session with api.proof-of-autonomy.elusaegis.xyz at 2025-08-29 13:31:04 UTC

📤 Messages sent:
POST /v1/chat/completions ...

📥 Messages received:
HTTP/1.1 200 OK
{"id":"chatcmpl-ebee...","model":"demo-gpt-4o-mini", ...}
```

---

## Development & Testing

To reproduce the CI setup locally:

1. Launch a dummy TLS model server with a self-signed cert (`model-server`).
2. Launch a local notary (`notary`).
3. Route the API domain to localhost (`/etc/hosts`).
4. Run the CLI with `prove` to create a proof.
5. Use the CLI again with `verify` to check the proof.

The CI scripts (`.github/workflows/ci.yml`) show an end-to-end setup with staged artifacts, TLS cert materialization,
and automatic verification of generated proofs.

## Sample Agent

This repository also includes a **sample agent** (`agent/`) that demonstrates how to integrate the library into an
autonomous workflow.  
The agent connects to a **dummy model server** and a **dummy notary**, requests contextual information (Polymarket +
portfolio snapshot), and produces a **decision request**.  
The execution is automatically notarized, so you get both:

1. The **decision output** (what the agent proposes).
2. A **cryptographic proof transcript** that the agent really saw the context and generated the output accordingly.

### Run the sample agent

{{{
cargo run --bin agent
}}}

Example output:

{{{
Polymarket context size: 1150 bytes
Portfolio context size: 1114 bytes
Decision request size: 2922 bytes
Success! ✅
}}}

This generates:

- a **JSON decision file** (agent’s reply), and
- a **proof transcript** under `proofs/` that you can later check with the CLI:

{{{
cargo run --bin cli -- verify proofs/<agent_proof_file>.json
}}}

This demonstrates the **end-to-end flow**:  
`agent` (application) → `cli` (proof generation library) → `verify` (cryptographic check).


---

## Roadmap

- **Distributed Notaries**: support for MPC- or TEE-backed notary pools.
- **AID integration**: export/import formal Agent Identity Documents for verified deployments.
- **Extended tools**: support for non-LLM APIs and compositional traces.
- **Proof explorers**: integrate with TLSNotary Explorer for public inspection.
- **Incident reporting**: link proofs to a shared ledger of AI misbehavior.

---

## References

- Grigor et al., *Agent Proofs: Scalable and Practical Verification of AI Autonomy*. ICML Workshop 2025.
- Grigor et al., *VET Your Agent: Towards Host-Independent Autonomy via Verifiable Execution Traces*. In Review.
- [TLSNotary Documentation](https://tlsnotary.org/about)

---
