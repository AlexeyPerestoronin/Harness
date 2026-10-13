//! Error type returned by configuration loading and template writing.

use std::fmt;

/// Failure while reading, parsing or validating configuration.
#[derive(Debug)]
pub struct ConfError {
    message: String,
}

impl ConfError {
    /// Build an error from a displayable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ConfError {}

impl From<std::io::Error> for ConfError {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<json5::Error> for ConfError {
    fn from(value: json5::Error) -> Self {
        Self::new(format!("json5 parse error: {value}"))
    }
}
