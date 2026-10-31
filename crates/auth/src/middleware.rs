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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(authorization: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = authorization {
            headers.insert(AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn missing_header_is_rejected() {
        let headers = headers(None);
        assert!(matches!(
            extract_bearer(&headers),
            Err(AuthError::MissingHeader)
        ));
    }

    #[test]
    fn invalid_scheme_is_rejected() {
        let headers = headers(Some("Basic dXNlcjpwYXNz"));
        assert!(matches!(
            extract_bearer(&headers),
            Err(AuthError::InvalidScheme)
        ));
    }

    #[test]
    fn malformed_bearer_is_rejected() {
        for value in ["Bearer", "Bearer ", "Bearer a b", "Bearer  token", "Bearer\tvalue"] {
            let headers = headers(Some(value));
            assert!(
                matches!(extract_bearer(&headers), Err(AuthError::InvalidScheme)),
                "unexpected success for {value:?}"
            );
        }
    }

    #[test]
    fn query_parameters_are_not_used() {
        // extract_bearer only inspects the Authorization header, so a token
        // placed in any other header or query parameter cannot be accepted.
        let headers = headers(None);
        assert!(matches!(
            extract_bearer(&headers),
            Err(AuthError::MissingHeader)
        ));
    }

    #[test]
    fn extracts_bearer_token() {
        let headers = headers(Some("Bearer test-token"));
        assert_eq!(extract_bearer(&headers).unwrap(), "test-token");
    }

    #[test]
    fn middleware_accepts_arc_token_state() {
        use axum::middleware::from_fn_with_state;
        use axum::routing::get;
        use axum::Router;

        let token = Arc::new(AuthToken::generate());
        let app: Router = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(from_fn_with_state(token, require_auth));

        let _ = app;
    }
}
