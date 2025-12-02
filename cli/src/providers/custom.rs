use super::{ExpectedChannelOverhead, Provider};

/// Custom provider that is initiated by the `model-server` crate.
#[derive(Debug, Clone, Default)]
pub struct Custom;

impl Custom {
    const REQUEST_OVERHEAD: usize = 250; // Estimated overhead for requests
    const RESPONSE_OVERHEAD: usize = 510; // Estimated overhead for responses
}

impl Provider for Custom {
    fn response_censor_headers(&self) -> &'static [&'static str] {
        &[]
    }

    /// Response headers to censor for privacy (default: common tracking headers)
    fn expected_overhead(&self) -> ExpectedChannelOverhead {
        ExpectedChannelOverhead::new(Some(Self::REQUEST_OVERHEAD), Some(Self::RESPONSE_OVERHEAD))
    }
}
