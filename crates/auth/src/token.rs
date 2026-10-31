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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_parse_verify_round_trip() {
        let token = AuthToken::generate();
        let encoded = token.as_base64url();

        let parsed = AuthToken::parse(&encoded).expect("generated token should parse");
        assert!(parsed.verify(&encoded));
        assert!(token.verify(&encoded));
    }

    #[test]
    fn generated_tokens_differ() {
        let first = AuthToken::generate();
        let second = AuthToken::generate();

        assert_ne!(first.as_base64url(), second.as_base64url());
        assert!(!first.verify(&second.as_base64url()));
        assert!(!second.verify(&first.as_base64url()));
    }

    #[test]
    fn verify_rejects_invalid_token() {
        let token = AuthToken::generate();

        assert!(!token.verify("not-base64"));
        assert!(!token.verify(""));
        assert!(!token.verify(&token.as_base64url()[..token.as_base64url().len() - 1]));
    }

    #[test]
    fn parse_rejects_invalid_length() {
        assert!(matches!(
            AuthToken::parse("c2hvcnQ"),
            Err(AuthError::InvalidToken)
        ));
    }
}
