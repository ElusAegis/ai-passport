//! Attestation proxy server for AI Passport.
//!
//! This proxy server forwards HTTP requests to backend APIs while recording
//! a transcript of all request/response pairs. Clients can request an attestation
//! of the transcript at the end of their session.
//!
//! ## Usage
//!
//! The proxy accepts HTTPS connections and forwards requests based on the `Host` header.
//! To get an attestation, send a request to `/__attest`.
//!
//! ## Example Flow
//!
//! ```text
//! 1. Client connects to proxy via TLS
//! 2. Client sends: POST /v1/messages, Host: api.anthropic.com
//! 3. Proxy forwards to api.anthropic.com, records request/response
//! 4. Client sends: GET /__attest, X-Censor-Headers: x-api-key
//! 5. Proxy returns JSON attestation with censored transcript
//! ```

pub mod proxy;
pub mod transcript;

pub use proxy::{run_server, ProxyConfig};
pub use transcript::{Attestation, TranscriptEntry};