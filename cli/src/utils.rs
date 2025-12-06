//! Shared utility functions.

use anyhow::Result;
use std::future::Future;
use std::time::Duration;

/// Wraps a future with an optional timeout.
/// If `timeout` is `None`, the future runs without a timeout.
pub async fn with_optional_timeout<F, T>(future: F, timeout: Option<Duration>) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match timeout {
        Some(duration) => tokio::time::timeout(duration, future)
            .await
            .map_err(|_| anyhow::anyhow!("Operation timed out after {:?}", duration))?,
        None => future.await,
    }
}