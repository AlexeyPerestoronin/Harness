//! [`NullLogger`]: no-op sink used by unit tests of tool modules.

use crate::Logger;

/// Logger that discards every record.
pub struct NullLogger;

impl Logger for NullLogger {
    fn log_str(&self, _message: &str) {}

    fn log_line(&self, _message: &str) {}
}
