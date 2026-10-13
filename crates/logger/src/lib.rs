//! Session logging for the harness loop, agents and tool modules.
//!
//! The crate exposes a small [`Logger`] trait plus two implementations:
//! [`DoubleLogger`] (stdout + `log.md`) and [`NullLogger`] (tests). Callers
//! hold `Arc<dyn Logger>` so the same instance can be shared across modules.

mod double;
mod null;

pub use double::DoubleLogger;
pub use null::NullLogger;

/// Thread-safe harness session journal.
///
/// Implementations duplicate each write to whatever sinks the caller needs
/// (stdout+file, in-memory buffer in tests, and so on).
pub trait Logger: Send + Sync {
    /// Write a fragment as-is, without forcing a newline.
    fn log_str(&self, message: &str);

    /// Write a line. If `message` does not start with `\n`, a `\n` prefix is added.
    fn log_line(&self, message: &str);
}
