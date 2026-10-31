//! Session logging for the harness loop, agents and tool modules.
//!
//! The crate exposes a single-method [`Logger`] contract plus three
//! implementations: [`FileLogger`] (append to a session log file),
//! [`DoubleLogger`] (stdout + file, kept for the legacy binary crate) and
//! [`NullLogger`] (tests). Callers hold `Arc<dyn Logger>` so the same
//! instance can be shared across modules.

mod double;
mod file;
mod null;

pub use double::DoubleLogger;
pub use file::FileLogger;
pub use null::NullLogger;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemLogger {
        text: Mutex<String>,
    }

    impl Logger for MemLogger {
        fn log_str(&self, message: &str, new_line: bool) {
            let mut text = self.text.lock().unwrap();
            if new_line && !message.starts_with('\n') {
                text.push('\n');
            }
            text.push_str(message);
        }
    }

    #[test]
    fn log_str_new_line_adds_prefix_when_needed() {
        let logger = MemLogger::default();

        logger.log_str("hello", true);
        logger.log_str("world", false);

        assert_eq!(logger.text.lock().unwrap().as_str(), "\nhelloworld");
    }

    #[test]
    fn log_str_new_line_keeps_existing_newline() {
        let logger = MemLogger::default();

        logger.log_str("\nhello", true);

        assert_eq!(logger.text.lock().unwrap().as_str(), "\nhello");
    }

    #[test]
    fn file_logger_creates_parent_dirs_and_appends() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("harness-logger-test-{unique}"));
        let path = dir.join("nested").join("session.log");

        let logger = FileLogger::new(&path).unwrap();
        logger.log_str("a", true);
        logger.log_str("b", false);
        drop(logger);

        let mut content = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "\nab");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
