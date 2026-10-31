//! Unified input/output contract for CLI and server sessions.
//!
//! The crate defines the [`Interaction`] trait used by agents and tools for
//! all user communication, [`OutputType`] which maps console output and file
//! logging, and the [`CliInteraction`] implementation backed by stdin/stdout.

mod cli;

use std::sync::Arc;

pub use cli::CliInteraction;
use logger::Logger;

/// Console/file output mode for [`Interaction::output`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    /// Continue writing at the current console position.
    ContinueLine,
    /// Start writing from a new line.
    NewLine,
    /// Rewrite the current console line (progress indicators).
    RewriteLine,
}

/// User interaction failure.
#[derive(Debug)]
pub enum InteractionError {
    /// Interaction was cancelled by the caller.
    Cancelled,
    /// Interaction timed out before an answer was provided.
    Timeout,
    /// Underlying I/O failure with a human-readable message.
    Io(String),
}

impl std::fmt::Display for InteractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("interaction cancelled"),
            Self::Timeout => f.write_str("interaction timed out"),
            Self::Io(message) => write!(f, "interaction I/O error: {message}"),
        }
    }
}

impl std::error::Error for InteractionError {}

/// Type of a user input request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionKind {
    /// Arbitrary text answer.
    Text,
    /// Confirmation answer (`y`/`n`).
    Confirm,
}

/// Description of a single user input request.
#[derive(Debug, Clone)]
pub struct InteractionRequest {
    /// The kind of answer expected from the user.
    pub kind: InteractionKind,
    /// Prompt shown to the user.
    pub prompt: String,
    /// Explicit flag for secret input (e.g. API keys).
    pub sensitive: bool,
}

/// File sink used by interaction implementations.
///
/// File records are written only when the caller passes `write_log = true`.
pub struct InteractionLoggers {
    /// File logger, usually `Some(FileLogger)`; `None` in tests.
    pub file: Option<Arc<dyn Logger>>,
}

impl InteractionLoggers {
    /// Build a logger set from an optional file sink.
    pub fn new(file: Option<Arc<dyn Logger>>) -> Self {
        Self { file }
    }
}

/// Asynchronous user input/output contract.
#[async_trait::async_trait]
pub trait Interaction: Send + Sync {
    /// Ask the user for a text answer and return it.
    async fn input(
        &self,
        request: &InteractionRequest,
        write_log: bool,
    ) -> Result<String, InteractionError>;

    /// Show information to the user.
    async fn output(
        &self,
        message: &str,
        write_log: bool,
        new_line: OutputType,
    ) -> Result<(), InteractionError>;
}
