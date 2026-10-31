//! [`DoubleLogger`]: stdout + session `log.md` sink.
//!
//! Kept temporarily so the legacy `harness` binary keeps compiling. New code
//! should use [`crate::FileLogger`] or [`crate::NullLogger`].

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use crate::Logger;

/// Logger that mirrors Python `DoubleLogger`: every record is written to
/// stdout and to the session log file.
pub struct DoubleLogger {
    file: Mutex<File>,
}

impl DoubleLogger {
    /// Create or touch `log_file` and open it for append.
    pub fn new(log_file: &Path) -> std::io::Result<Self> {
        if let Some(parent) = log_file.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl Logger for DoubleLogger {
    fn log_str(&self, message: &str, new_line: bool) {
        let text = if new_line && !message.starts_with('\n') {
            format!("\n{message}")
        } else {
            message.to_string()
        };
        print!("{text}");
        let _ = std::io::stdout().flush();
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(text.as_bytes());
            let _ = file.flush();
        }
    }
}
