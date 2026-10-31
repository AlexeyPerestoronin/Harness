//! Error type returned by configuration loading and template writing.
//!
//! Besides ordinary validation messages, `ConfError` carries two machine
//! readable variants used by the server when it must ask the user to choose
//! an agent: [`ConfErrorKind::AgentChoiceRequired`] and
//! [`ConfErrorKind::InvalidAgentChoice`].

use std::fmt;

use crate::types::AgentCandidate;

/// Programmatically distinguishable configuration error kind.
#[derive(Debug)]
pub enum ConfErrorKind {
    /// Ordinary validation / IO / parse error with a human-readable message.
    Message(String),
    /// Several agent candidates were found and the caller must choose one.
    AgentChoiceRequired(Vec<AgentCandidate>),
    /// The supplied agent choice is not a number in the valid range.
    InvalidAgentChoice(String),
}

/// Failure while reading, parsing or validating configuration.
#[derive(Debug)]
pub struct ConfError {
    kind: ConfErrorKind,
}

impl ConfError {
    /// Build an ordinary error from a displayable message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            kind: ConfErrorKind::Message(message.into()),
        }
    }

    /// Build an error that signals a required agent choice.
    pub fn agent_choice_required(candidates: Vec<AgentCandidate>) -> Self {
        Self {
            kind: ConfErrorKind::AgentChoiceRequired(candidates),
        }
    }

    /// Build an error for an invalid 1-based agent choice.
    pub fn invalid_agent_choice(message: impl Into<String>) -> Self {
        Self {
            kind: ConfErrorKind::InvalidAgentChoice(message.into()),
        }
    }

    /// Expose the machine-readable error kind.
    pub fn kind(&self) -> &ConfErrorKind {
        &self.kind
    }

    /// `true` when the caller must ask the user to choose an agent.
    pub fn is_agent_choice_required(&self) -> bool {
        matches!(self.kind, ConfErrorKind::AgentChoiceRequired(_))
    }

    /// `true` when the supplied agent choice was not a valid 1-based number.
    pub fn is_invalid_agent_choice(&self) -> bool {
        matches!(self.kind, ConfErrorKind::InvalidAgentChoice(_))
    }
}

impl fmt::Display for ConfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ConfErrorKind::Message(message) => write!(f, "{message}"),
            ConfErrorKind::InvalidAgentChoice(message) => write!(f, "{message}"),
            ConfErrorKind::AgentChoiceRequired(candidates) => {
                write!(f, "agent choice required:")?;
                for (index, candidate) in candidates.iter().enumerate() {
                    write!(
                        f,
                        "\n{}) {} — {}",
                        index + 1,
                        candidate.vendor,
                        candidate.name
                    )?;
                }
                Ok(())
            }
        }
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
