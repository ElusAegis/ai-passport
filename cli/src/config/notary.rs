use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use tlsn_common::config::NetworkSetting;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum NotaryMode {
    Ephemeral,
    RemoteNonTLS,
    RemoteTLS,
}

#[derive(Builder, Clone, Debug, Serialize, Deserialize)]
#[builder(pattern = "owned")]
pub struct NotaryConfig {
    /// The domain of the notary server
    pub domain: String,
    /// The port of the notary server
    #[builder(setter(into))]
    pub port: u16,
    /// The route for notary requests
    #[builder(setter(into))]
    pub path_prefix: String,
    /// Notary type
    #[builder(default = "NotaryMode::Ephemeral")]
    pub mode: NotaryMode,
    /// Maximum total number of bytes sent over the whole session
    pub max_total_sent: usize,
    /// Maximum total number of bytes received over the whole session
    pub max_total_recv: usize,
    /// Defer decryption of messages until the end of the session
    #[builder(default = "true")]
    pub defer_decryption: bool,
    /// Maximum total number of messages decrypted in the online phase
    #[builder(default = "0")]
    pub max_decrypted_online: usize,
    /// Network optimization strategy
    #[builder(default)]
    pub network_optimization: NetworkSetting,
}

impl NotaryConfig {
    pub fn builder() -> NotaryConfigBuilder {
        NotaryConfigBuilder::default()
    }

    /// Create a copy with updated total_sent value
    pub fn with_total_sent(&self, total_sent: usize) -> Self {
        let mut new = self.clone();
        new.max_total_sent = total_sent;
        new
    }
}
