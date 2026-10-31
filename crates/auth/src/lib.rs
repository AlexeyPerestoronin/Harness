//! Bearer-token authentication for the axum server.
//!
//! The crate generates and verifies base64url bearer tokens and exposes the
//! [`require_auth`] middleware, which is the single protection layer for all
//! protected routes.

mod error;
mod middleware;
mod token;

pub use error::{AuthError, ErrorBody, ErrorResponse};
pub use middleware::{extract_bearer, require_auth};
pub use token::AuthToken;
