//! TLS configuration loading.

use anyhow::{bail, Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use rustls::crypto::aws_lc_rs::default_provider;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{version, ServerConfig};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;

/// Load the first private key found in `path` (PKCS#8 -> PKCS#1 -> SEC1).
fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    // Try PKCS#8 first
    {
        let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
        let keys: Vec<_> = pkcs8_private_keys(&mut r).flatten().collect();
        if let Some(key) = keys.into_iter().next() {
            return Ok(PrivateKeyDer::from(key));
        }
    }
    // Then PKCS#1 (RSA)
    {
        let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
        let keys: Vec<_> = rsa_private_keys(&mut r).flatten().collect();
        if let Some(key) = keys.into_iter().next() {
            return Ok(PrivateKeyDer::from(key));
        }
    }

    bail!("no private key found in {path} (tried PKCS#8, PKCS#1, SEC1)");
}

/// Load a cert chain into rustls-compatible types.
fn load_cert_chain(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let mut r = BufReader::new(File::open(path).with_context(|| format!("open {}", path))?);
    certs(&mut r)
        .collect::<Result<Vec<CertificateDer>, _>>()
        .map_err(Into::into)
}

/// Create a RustlsConfig from certificate and key file paths.
///
/// Configures TLS 1.2 and TLS 1.3 with ALPN for HTTP/2 and HTTP/1.1.
pub async fn rustls_config_from_paths(cert_path: &str, key_path: &str) -> Result<RustlsConfig> {
    let certs = load_cert_chain(cert_path)?;
    let key = load_private_key(key_path)?;

    // Explicitly select TLS versions: TLS1.3 *and* TLS1.2
    let provider = default_provider();
    let mut config = ServerConfig::builder_with_provider(<Arc<CryptoProvider>>::from(provider))
        .with_protocol_versions(&[&version::TLS13, &version::TLS12])?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    // Advertise ALPN for h2 and http/1.1
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(RustlsConfig::from_config(Arc::new(config)))
}
