//! [`AuthToken`] generation, parsing and constant-time verification.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;

use crate::error::AuthError;

const TOKEN_LEN: usize = 32;

/// Bearer token stored in server memory.
#[derive(Clone, Debug)]
pub struct AuthToken {
    secret: [u8; TOKEN_LEN],
}

impl AuthToken {
    /// Generate a new random token (32 bytes, base64url without padding).
    pub fn generate() -> Self {
        let mut secret = [0u8; TOKEN_LEN];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        Self { secret }
    }

    /// Decode and validate a base64url token.
    pub fn parse(input: &str) -> Result<Self, AuthError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(input)
            .map_err(|_| AuthError::InvalidToken)?;
        let secret: [u8; TOKEN_LEN] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| AuthError::InvalidToken)?;
        Ok(Self { secret })
    }

    /// Encode the token as base64url without padding.
    pub fn as_base64url(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.secret)
    }

    /// Constant-time comparison of `candidate` against this token.
    pub fn verify(&self, candidate: &str) -> bool {
        let Ok(bytes) = URL_SAFE_NO_PAD.decode(candidate) else {
            return false;
        };
        if bytes.len() != TOKEN_LEN {
            return false;
        }

        let mut diff = 0u8;
        for (expected, actual) in self.secret.iter().zip(bytes.iter()) {
            diff |= expected ^ actual;
        }
        diff == 0
    }
}

