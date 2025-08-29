use crate::config::SessionConfig;
use crate::SessionMode;
use anyhow::{Context, Result};
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use tlsn_common::config::NetworkSetting;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum NotaryMode {
    Ephemeral,
    RemoteNonTLS,
    RemoteTLS,
}

#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct NotaryConfig {
    /// The domain of the notary server
    pub(crate) domain: String,
    /// The port of the notary server
    #[builder(setter(into))]
    pub(crate) port: u16,
    /// The route for notary requests
    #[builder(setter(into))]
    pub(crate) path_prefix: String,
    /// Notary type
    #[builder(default = "NotaryMode::Ephemeral")]
    pub(crate) mode: NotaryMode,
    /// Maximum total number of bytes sent over the whole session (default to equal single request size)
    pub max_total_sent: usize,
    /// Maximum total number of bytes received over the whole session (default to equal single response size)
    pub max_total_recv: usize,
    /// Defer decryption of messages until the end of the session
    #[builder(default = "true")]
    pub(crate) defer_decryption: bool,
    /// Maximum total number of messages decrypted in the online phase
    #[builder(default = "0")]
    pub(crate) max_decrypted_online: usize,
    /// Network optimization strategy
    #[builder(default)]
    pub(crate) network_optimization: NetworkSetting,
}

impl NotaryConfig {
    pub fn builder() -> NotaryConfigBuilder {
        NotaryConfigBuilder::default()
    }

    pub fn set_total_sent(&self, total_sent: usize) -> NotaryConfig {
        NotaryConfigBuilder::default()
            .domain(self.domain.clone())
            .port(self.port)
            .path_prefix(self.path_prefix.clone())
            .mode(self.mode)
            .max_total_sent(total_sent)
            .max_total_recv(self.max_total_recv)
            .defer_decryption(self.defer_decryption)
            .max_decrypted_online(self.max_decrypted_online)
            .network_optimization(self.network_optimization)
            .build()
            .expect("Failed to build NotaryConfig")
    }
}

impl NotaryConfigBuilder {
    pub fn finalize_for_session(
        mut self: NotaryConfigBuilder,
        config: &SessionConfig,
    ) -> Result<NotaryConfig> {
        let (total_sent, total_recv) = if matches!(config.mode, SessionMode::Multi) {
            // --- One‑shot: exact, per‑round sizing --------------------------------
            //
            // We create a new protocol instance per request. We already know (or can
            // compute) precise sizes for this single request/response.
            // This is done before we invoke the setup.
            (
                config.max_total_single_request_size,
                config.max_total_response_size,
            )
        } else {
            // --- Multi‑round: stateless model API; sizes grow with history ----------
            //
            // Let:
            //   n   = max number of requests sent to the model API
            //   rsp = max_single_response_size (upper bound per response)
            //   req = max_single_request_size (upper bound per request)
            //
            // Because each new request re-sends prior context, cumulative *sent*
            // bytes across the session follow an arithmetic series that simplifies to:
            //
            //   total_sent_estimate = (req * (n - 1) * n + rsp * (n - 1) * (n - 2)) / 2
            //   total_recv_estimate = rsp * n
            let (total_sent_max, total_recv_max) = config.max_total_sent_recv();

            self = self
                .defer_decryption(false)
                .max_decrypted_online(total_recv_max);

            (total_sent_max, total_recv_max)
        };

        self.max_total_sent(total_sent)
            .max_total_recv(total_recv)
            .build()
            .context("Error building Notary configuration")
    }
}
