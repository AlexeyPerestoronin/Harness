//! Session logging for the harness loop, agents and tool modules.
//!
//! The crate exposes a single-method [`Logger`] contract plus the
//! [`FileLogger`] implementation (append to a session log file). Callers hold
//! `Arc<dyn Logger>` so the same instance can be shared across modules.

mod file;

pub use file::FileLogger;

/// Thread-safe harness session journal.
///
/// Implementations write each fragment to whatever sinks the caller needs
/// (file, stdout + file, in-memory buffer in tests, and so on).
pub trait Logger: Send + Sync {
    /// Write a fragment `message`.
    ///
    /// `new_line = true` starts the record on a fresh line: if `message`
    /// does not start with `\n`, a `\n` prefix is added.
    /// `new_line = false` writes `message` exactly as supplied.
    fn log_str(&self, message: &str, new_line: bool);
}
