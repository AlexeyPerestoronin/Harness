//! [`CliInteraction`]: stdin/stdout implementation of [`Interaction`].

use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

use crate::{Interaction, InteractionError, InteractionLoggers, InteractionRequest, OutputType};

type SharedReader = Arc<Mutex<Box<dyn BufRead + Send>>>;
type SharedWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Console interaction backed by stdin and stdout.
///
/// File logging is delegated to [`InteractionLoggers`] and happens only when
/// `write_log` is `true`. The reader/writer can be injected for unit tests.
pub struct CliInteraction {
    loggers: InteractionLoggers,
    reader: SharedReader,
    writer: SharedWriter,
}

impl CliInteraction {
    /// Build a CLI interaction using real stdin and stdout.
    pub fn new(loggers: InteractionLoggers) -> Self {
        Self::with_reader_writer(loggers, BufReader::new(io::stdin()), io::stdout())
    }

    /// Build a CLI interaction with injected reader and writer.
    pub fn with_reader_writer<R, W>(loggers: InteractionLoggers, reader: R, writer: W) -> Self
    where
        R: BufRead + Send + 'static,
        W: Write + Send + 'static,
    {
        Self {
            loggers,
            reader: Arc::new(Mutex::new(Box::new(reader))),
            writer: Arc::new(Mutex::new(Box::new(writer))),
        }
    }

    fn write_stdout(&self, message: &str, new_line: OutputType) -> Result<(), InteractionError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| InteractionError::Io("stdout lock poisoned".to_string()))?;
        let text = match new_line {
            OutputType::ContinueLine => message.to_string(),
            OutputType::NewLine => {
                if message.starts_with('\n') {
                    message.to_string()
                } else {
                    format!("\n{message}")
                }
            }
            OutputType::RewriteLine => format!("\r{message}"),
        };
        writer
            .write_all(text.as_bytes())
            .map_err(|e| InteractionError::Io(e.to_string()))?;
        writer
            .flush()
            .map_err(|e| InteractionError::Io(e.to_string()))
    }

    fn write_file(&self, message: &str, new_line: bool) {
        if let Some(file) = &self.loggers.file {
            file.log_str(message, new_line);
        }
    }
}

#[async_trait::async_trait]
impl Interaction for CliInteraction {
    async fn output(
        &self,
        message: &str,
        write_log: bool,
        new_line: OutputType,
    ) -> Result<(), InteractionError> {
        self.write_stdout(message, new_line)?;
        if write_log {
            match new_line {
                OutputType::ContinueLine | OutputType::RewriteLine => {
                    self.write_file(message, false)
                }
                OutputType::NewLine => self.write_file(message, true),
            }
        }
        Ok(())
    }

    async fn input(
        &self,
        request: &InteractionRequest,
        write_log: bool,
    ) -> Result<String, InteractionError> {
        self.write_stdout(&request.prompt, OutputType::ContinueLine)?;

        let reader = Arc::clone(&self.reader);
        let answer = tokio::task::spawn_blocking(move || {
            let mut reader = reader
                .lock()
                .map_err(|_| InteractionError::Io("stdin lock poisoned".to_string()))?;
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| InteractionError::Io(e.to_string()))?;
            Ok::<_, InteractionError>(line)
        })
        .await
        .map_err(|e| InteractionError::Io(e.to_string()))??;

        let answer = answer
            .trim_end_matches(|c| c == '\r' || c == '\n')
            .to_string();

        if write_log {
            self.write_file(&request.prompt, true);
            self.write_file(&answer, true);
        }

        Ok(answer)
    }
}

