//! Authentication middleware.

use crate::config::Config;
use axum::extract::Request;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Middleware to require API key authentication.
///
/// If `config.api_key` is set, requires `Authorization: Bearer <key>` header.
/// Uses constant-time comparison to prevent timing attacks.
pub async fn require_api_key(
    State(cfg): State<Arc<Config>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    if let Some(expected) = &cfg.api_key {
        let auth = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");

        // Must be "Bearer <token>"
        let Some(provided) = auth.strip_prefix("Bearer ") else {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid API key"));
        };

        // Constant-time comparison to avoid timing side-channels
        let ok: bool = provided.as_bytes().ct_eq(expected.as_bytes()).into();

        if !ok {
            return Err((StatusCode::UNAUTHORIZED, "missing or invalid API key"));
        }
    }
    Ok(next.run(req).await)
}
