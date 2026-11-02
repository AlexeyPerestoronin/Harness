//! Axum middleware protecting routes with a Bearer token.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AuthError;
use crate::token::AuthToken;

/// Extract exactly `Bearer <token>` from the `Authorization` header.
///
/// Query parameters and cookies are intentionally not considered.
pub fn extract_bearer(headers: &HeaderMap) -> Result<&str, AuthError> {
    let value = headers.get(AUTHORIZATION).ok_or(AuthError::MissingHeader)?;
    let value = value.to_str().map_err(|_| AuthError::InvalidScheme)?;
    let (scheme, token) = value.split_once(' ').ok_or(AuthError::InvalidScheme)?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return Err(AuthError::InvalidScheme);
    }
    if token.is_empty() || token.chars().any(|c| c.is_whitespace()) {
        return Err(AuthError::InvalidScheme);
    }
    Ok(token)
}

/// Axum middleware: the only authentication mechanism for protected routes.
///
/// The state type is intentionally simplified to `Arc<AuthToken>` so this
/// crate does not depend on a concrete server state type.
pub async fn require_auth(
    State(token): State<Arc<AuthToken>>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let candidate = extract_bearer(request.headers())?;
    if !token.verify(candidate) {
        return Err(AuthError::Unauthorized);
    }
    Ok(next.run(request).await)
}

