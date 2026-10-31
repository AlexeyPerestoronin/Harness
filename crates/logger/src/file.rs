//! [`FileLogger`]: append-only file sink used by CLI and server sessions.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use crate::Logger;

/// Logger that appends every record to a single log file.
pub struct FileLogger {
    file: Mutex<File>,
}

impl FileLogger {
    /// Create parent directories (when needed) and open `path` for append.
    pub fn new(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl Logger for FileLogger {
    fn log_str(&self, message: &str, new_line: bool) {
        if let Ok(mut file) = self.file.lock() {
            if new_line && !message.starts_with('\n') {
                let _ = file.write_all(b"\n");
            }
            let _ = file.write_all(message.as_bytes());
            let _ = file.flush();
        }
    }
}
