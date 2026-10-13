//! [`DoubleLogger`]: every record is written to stdout and to the session `log.md`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use crate::Logger;

/// Logger that mirrors Python `DoubleLogger`: touch the file, then append
/// every record to both stdout and the file.
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
    fn log_str(&self, message: &str) {
        print!("{message}");
        let _ = std::io::stdout().flush();
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(message.as_bytes());
            let _ = file.flush();
        }
    }

    fn log_line(&self, message: &str) {
        let owned;
        let text = if message.starts_with('\n') {
            message
        } else {
            owned = format!("\n{message}");
            owned.as_str()
        };
        self.log_str(text);
    }
}
