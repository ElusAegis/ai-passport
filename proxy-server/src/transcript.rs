//! Transcript types for recording HTTP request/response pairs.

use k256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde::{Deserialize, Serialize};

/// A single entry in the transcript (request or response).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "direction", rename_all = "lowercase")]
pub enum TranscriptEntry {
    Request {
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: String,
    },
    Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
    },
}

impl TranscriptEntry {
    /// Create a transcript entry from an HTTP request.
    pub fn request(
        method: String,
        path: String,
        headers: Vec<(String, String)>,
        body: String,
    ) -> Self {
        Self::Request {
            method,
            path,
            headers,
            body,
        }
    }

    /// Create a transcript entry from an HTTP response.
    pub fn response(status: u16, headers: Vec<(String, String)>, body: String) -> Self {
        Self::Response {
            status,
            headers,
            body,
        }
    }

    /// Censor specified headers by replacing their values with X's.
    pub fn censor_headers(&mut self, headers_to_censor: &[String]) {
        let headers = match self {
            Self::Request { headers, .. } => headers,
            Self::Response { headers, .. } => headers,
        };

        for (name, value) in headers {
            if headers_to_censor
                .iter()
                .any(|h| h.eq_ignore_ascii_case(name))
            {
                *value = "X".repeat(value.len());
            }
        }
    }
}

/// The unsigned attestation data that gets signed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnsignedAttestation {
    /// The target host that was proxied to.
    pub target_host: String,
    /// Timestamp when attestation was generated.
    pub timestamp: String,
    /// The recorded transcript (censored).
    pub transcript: Vec<TranscriptEntry>,
}

/// The attestation returned to the client, including a signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attestation {
    /// The target host that was proxied to.
    pub target_host: String,
    /// Timestamp when attestation was generated.
    pub timestamp: String,
    /// The recorded transcript (censored).
    pub transcript: Vec<TranscriptEntry>,
    /// ECDSA signature over the attestation data (hex-encoded).
    pub signature: String,
}

impl Attestation {
    /// Build an attestation from a session's transcript and sign it.
    pub fn build_and_sign(
        mut transcript: Vec<TranscriptEntry>,
        target_host: String,
        censor_headers: &[String],
        signing_key: &SigningKey,
    ) -> Self {
        for entry in &mut transcript {
            entry.censor_headers(censor_headers);
        }

        let timestamp = chrono::Utc::now().to_rfc3339();

        // Create unsigned attestation for signing
        let unsigned = UnsignedAttestation {
            target_host: target_host.clone(),
            timestamp: timestamp.clone(),
            transcript: transcript.clone(),
        };

        // Serialize to canonical JSON and sign
        let message = serde_json::to_string(&unsigned).expect("Failed to serialize attestation");
        let signature: Signature = signing_key.sign(message.as_bytes());
        let signature_hex = hex::encode(signature.to_bytes());

        Self {
            target_host,
            timestamp,
            transcript,
            signature: signature_hex,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_censor_headers() {
        let mut entry = TranscriptEntry::request(
            "POST".into(),
            "/v1/messages".into(),
            vec![
                ("content-type".into(), "application/json".into()),
                ("x-api-key".into(), "sk-secret-key-12345".into()),
                ("Authorization".into(), "Bearer token123".into()),
            ],
            "{}".into(),
        );

        entry.censor_headers(&["x-api-key".into(), "authorization".into()]);

        if let TranscriptEntry::Request { headers, .. } = entry {
            assert_eq!(headers[0].1, "application/json");
            assert_eq!(headers[1].1, "XXXXXXXXXXXXXXXXXXX"); // 19 X's for "sk-secret-key-12345"
            assert_eq!(headers[2].1, "XXXXXXXXXXXXXXX"); // 15 X's for "Bearer token123"
        } else {
            panic!("Expected Request variant");
        }
    }
}
