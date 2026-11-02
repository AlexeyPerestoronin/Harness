//! [`WebInteraction`]: server implementation of the [`Interaction`] contract.
//!
//! Output is never written to stdout; when `write_log = true` records go to
//! the session [`FileLogger`]. Input requests are stored as `pendingInput` in
//! the active run and wait for an HTTP answer through a one-shot channel.

use std::sync::Arc;
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use harness_api::{PendingInput, PendingInputKind, RunState};
use interactions::{
    Interaction, InteractionError, InteractionKind, InteractionLoggers, InteractionRequest,
    OutputType,
};
use tokio::sync::oneshot;

use super::state::{ServerState, UserResponse};

/// Server-side interaction backed by [`ServerState`] and an optional file logger.
pub struct WebInteraction {
    state: Arc<ServerState>,
    loggers: InteractionLoggers,
}

impl WebInteraction {
    /// Build a web interaction.
    ///
    /// `loggers.file` is `Some(FileLogger)` for the session log and `None`
    /// for pre-session questions (agent selection, API-key prompt).
    pub fn new(state: Arc<ServerState>, loggers: InteractionLoggers) -> Self {
        Self { state, loggers }
    }

    async fn interaction_timeout(&self) -> Duration {
        let minutes = self
            .state
            .config
            .read()
            .await
            .server
            .interaction_timeout_minutes;
        Duration::from_secs(minutes.saturating_mul(60))
    }

    fn write_file(&self, message: &str, new_line: bool) {
        if let Some(file) = &self.loggers.file {
            file.log_str(message, new_line);
        }
    }

    fn pending_kind(kind: InteractionKind) -> PendingInputKind {
        match kind {
            InteractionKind::Text => PendingInputKind::Text,
            InteractionKind::Confirm => PendingInputKind::Confirm,
        }
    }
}

#[async_trait::async_trait]
impl Interaction for WebInteraction {
    async fn output(
        &self,
        message: &str,
        write_log: bool,
        new_line: OutputType,
    ) -> Result<(), InteractionError> {
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
        let timeout = self.interaction_timeout().await;
        let request_id = format!("req_{}", ulid::Ulid::new());
        let timeout_at = (Utc::now()
            + chrono::Duration::from_std(timeout).map_err(|_| {
                InteractionError::Io("interaction timeout is too large".to_string())
            })?)
        .to_rfc3339_opts(SecondsFormat::Secs, true);

        let pending = PendingInput {
            request_id: request_id.clone(),
            kind: Self::pending_kind(request.kind),
            prompt: request.prompt.clone(),
            timeout_at,
            sensitive: request.sensitive,
        };

        let (tx, rx) = oneshot::channel();
        let cancel = {
            let mut active = self.state.active.lock().await;
            let active = active.as_mut().ok_or_else(|| {
                InteractionError::Io("no active run to attach the input to".to_string())
            })?;
            active.pending_input = Some(pending);
            active.input_tx = Some(tx);
            active.state = RunState::WaitingInput;
            active.updated_at = Utc::now();
            active.cancel.clone()
        };

        let result = tokio::time::timeout(timeout, async {
            tokio::select! {
                _ = cancel.cancelled() => Err(InteractionError::Cancelled),
                answer = rx => match answer {
                    Ok(UserResponse::Answer(answer)) => Ok(answer),
                    Err(_) => Err(InteractionError::Cancelled),
                },
            }
        })
        .await;

        let next_state = match &result {
            Ok(Ok(_)) => RunState::Running,
            Ok(Err(InteractionError::Cancelled)) => RunState::Stopping,
            Ok(Err(_)) => RunState::Running,
            Err(_) => RunState::Running,
        };

        let mut active = self.state.active.lock().await;
        if let Some(active) = active.as_mut() {
            active.pending_input = None;
            active.input_tx = None;
            active.updated_at = Utc::now();
            active.state = next_state;
        }

        match result {
            Ok(Ok(answer)) => {
                if write_log {
                    self.write_file(&request.prompt, true);
                    self.write_file(&answer, true);
                }
                Ok(answer)
            }
            Ok(Err(error)) => Err(error),
            Err(_) => Err(InteractionError::Timeout),
        }
    }
}

