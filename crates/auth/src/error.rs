//! Auth errors and their HTTP/JSON mapping.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// Machine-readable error body returned for auth failures.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// Stable error code.
    pub code: &'static str,
    /// Human-readable error message.
    pub message: &'static str,
}

/// JSON envelope for an auth error.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Nested error object required by the REST contract.
    pub error: ErrorBody,
}

/// Authentication failures.
#[derive(Debug)]
pub enum AuthError {
    /// `Authorization` header is absent.
    MissingHeader,
    /// Scheme is not exactly `Bearer <token>`.
    InvalidScheme,
    /// Token cannot be decoded as 32 base64url bytes.
    InvalidToken,
    /// Token does not match the server token.
    Unauthorized,
}

impl AuthError {
    /// HTTP status for every auth failure.
    pub fn status(&self) -> StatusCode {
        StatusCode::UNAUTHORIZED
    }

    /// Error body for every auth failure.
    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            code: "UNAUTHORIZED",
            message: "missing or invalid bearer token",
        }
    }

    /// JSON envelope for every auth failure.
    pub fn error_response(&self) -> ErrorResponse {
        ErrorResponse { error: self.body() }
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (self.status(), Json(self.error_response())).into_response()
    }
}
