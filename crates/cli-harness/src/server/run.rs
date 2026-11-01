//! Background task started by `POST /api/v1/run`.
//!
//! The task resolves the concrete agent through
//! [`conf::resolve_agent_for_run`], asks for an agent choice or an API key
//! through [`WebInteraction`] when needed, creates the session log/dump and
//! runs the asynchronous agent loop. Results are written back to
//! [`ActiveRun`].

use std::sync::Arc;

use agent_loop::{
    AgentLoop, RunObserver, RunOutcome, RunState as LoopRunState, RunSummary as LoopSummary,
};
use chrono::Utc;
use conf::ServerSettings;
use harness_api::{RunResult, RunState, RunStats, RunSummary};
use interactions::{
    Interaction, InteractionError, InteractionKind, InteractionLoggers, InteractionRequest,
};
use logger::{FileLogger, Logger};
use tokio_util::sync::CancellationToken;

use super::state::ServerState;
use super::web_interaction::WebInteraction;

/// Spawned by the start-run handler. Always writes a terminal result into
/// the matching [`ActiveRun`] slot.
pub async fn run_agent_task(
    state: Arc<ServerState>,
    run_id: String,
    prompt: String,
    cancel: CancellationToken,
) {
    let outcome = run_inner(state.clone(), &run_id, prompt, cancel).await;
    write_terminal_result(&state, &run_id, &outcome).await;
}

/// Failure kind for the pre-session resolution step.
enum ResolveAgentError {
    /// The run was cancelled while asking for an agent choice or an API key.
    Cancelled,
    /// The resolution failed for any other reason.
    Failed(String),
}

async fn run_inner(
    state: Arc<ServerState>,
    run_id: &str,
    prompt: String,
    cancel: CancellationToken,
) -> RunOutcome {
    let loaded = {
        let config = state.config.read().await;
        config.clone()
    };
    let Some(settings) = loaded.settings.clone() else {
        return failed_outcome("configuration is not available");
    };

    let pre_session: Arc<dyn Interaction> = Arc::new(WebInteraction::new(
        state.clone(),
        InteractionLoggers::new(None),
    ));

    let resolved = match resolve_agent_for_run_interactive(&settings, &pre_session).await {
        Ok(resolved) => resolved,
        Err(ResolveAgentError::Cancelled) => return stopped_outcome(),
        Err(ResolveAgentError::Failed(error)) => return failed_outcome(error),
    };

    let source = source_with_agent_config(&settings, &resolved.config_path);
    let session_dir = match agent_loop::session_dir(
        &settings.cwd,
        &settings.agent_session.log_folder,
        &resolved.agent.vendor,
        &resolved.agent.name,
    ) {
        Ok(dir) => dir,
        Err(error) => return failed_outcome(error),
    };
    let log_file = session_dir.join("log.md");
    let dump_file = session_dir.join("dump.json");

    let file: Arc<dyn Logger> = match FileLogger::new(&log_file) {
        Ok(file) => Arc::new(file),
        Err(error) => {
            return failed_outcome(format!("cannot open log '{}': {error}", log_file.display()))
        }
    };
    let interaction: Arc<dyn Interaction> = Arc::new(WebInteraction::new(
        state.clone(),
        InteractionLoggers::new(Some(file)),
    ));

    let agent_loop = match AgentLoop::new(
        settings.cwd.clone(),
        settings.agent_session.log_folder.clone(),
        resolved.agent.clone(),
        settings.tools.clone(),
        interaction,
        session_dir,
    ) {
        Ok(agent_loop) => agent_loop,
        Err(error) => return failed_outcome(error),
    };
    let mut agent_loop = agent_loop.with_dump_settings(source);
    let observer: Arc<dyn RunObserver> = Arc::new(ServerRunObserver {
        state: state.clone(),
        run_id: run_id.to_string(),
    });

    match agent_loop
        .run(Some(prompt), &dump_file, None, cancel, Some(observer))
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => failed_outcome(error),
    }
}

async fn resolve_agent_for_run_interactive(
    settings: &ServerSettings,
    interaction: &Arc<dyn Interaction>,
) -> Result<conf::ResolvedAgent, ResolveAgentError> {
    let mut choice: Option<String> = None;
    let mut api_key_override: Option<String> = None;

    loop {
        match conf::resolve_agent_for_run(settings, choice.as_deref(), api_key_override.as_deref())
        {
            Ok(resolved) => {
                if resolved.agent.api_key.is_empty() && api_key_override.is_none() {
                    let request = InteractionRequest {
                        kind: InteractionKind::Text,
                        prompt: format!(
                            "Enter API key for {} {}: ",
                            resolved.agent.vendor, resolved.agent.name
                        ),
                        sensitive: true,
                    };
                    match interaction.input(&request, false).await {
                        Ok(key) => {
                            if key.trim().is_empty() {
                                return Err(ResolveAgentError::Failed(
                                    "api-key is required".to_string(),
                                ));
                            }
                            api_key_override = Some(key);
                            continue;
                        }
                        Err(InteractionError::Cancelled) => {
                            return Err(ResolveAgentError::Cancelled)
                        }
                        Err(error) => {
                            return Err(ResolveAgentError::Failed(format!(
                                "cannot prompt for api key: {error}"
                            )))
                        }
                    }
                }
                return Ok(resolved);
            }
            Err(error) if error.is_agent_choice_required() => {
                let candidates = match error.kind() {
                    conf::ConfErrorKind::AgentChoiceRequired(candidates) => candidates.clone(),
                    _ => return Err(ResolveAgentError::Failed(error.to_string())),
                };
                let mut selected: Option<usize> = None;
                let mut cancelled = false;
                for _ in 0..3 {
                    let request = InteractionRequest {
                        kind: InteractionKind::Text,
                        prompt: agent_loop::agent_select::build_prompt(&candidates),
                        sensitive: false,
                    };
                    match interaction.input(&request, true).await {
                        Ok(answer) => {
                            if let Some(index) = agent_loop::agent_select::parse_choice(
                                answer.trim(),
                                candidates.len(),
                            ) {
                                selected = Some(index);
                                break;
                            }
                        }
                        Err(InteractionError::Cancelled) => {
                            cancelled = true;
                            break;
                        }
                        Err(error) => {
                            return Err(ResolveAgentError::Failed(format!(
                                "cannot select agent -> {error}"
                            )));
                        }
                    }
                }
                if cancelled {
                    return Err(ResolveAgentError::Cancelled);
                }
                match selected {
                    Some(index) => choice = Some((index + 1).to_string()),
                    None => {
                        return Err(ResolveAgentError::Failed(
                            "agent was not selected".to_string(),
                        ))
                    }
                }
            }
            Err(error) => return Err(ResolveAgentError::Failed(error.to_string())),
        }
    }
}

/// Observer that mirrors agent-loop state and summary changes into [`ActiveRun`].
struct ServerRunObserver {
    state: Arc<ServerState>,
    run_id: String,
}

#[async_trait::async_trait]
impl RunObserver for ServerRunObserver {
    async fn on_state(&self, state: LoopRunState) {
        let mut active = self.state.active.lock().await;
        if let Some(run) = active.as_mut() {
            if run.run_id == self.run_id {
                run.state = map_run_state(state);
                run.updated_at = Utc::now();
            }
        }
    }

    async fn on_summary(&self, summary: LoopSummary) {
        let mut active = self.state.active.lock().await;
        if let Some(run) = active.as_mut() {
            if run.run_id == self.run_id {
                run.summary = map_summary(&summary);
                run.updated_at = Utc::now();
            }
        }
    }
}

async fn write_terminal_result(state: &Arc<ServerState>, run_id: &str, outcome: &RunOutcome) {
    let terminal_state = map_run_state(outcome.state);
    let mut active = state.active.lock().await;
    if let Some(run) = active.as_mut() {
        if run.run_id == run_id {
            run.state = terminal_state;
            run.summary = map_summary(&outcome.summary);
            run.result = Some(RunResult {
                ok: outcome.state == LoopRunState::Finished,
                run_state: terminal_state,
                stats: RunStats {
                    iteration: outcome.summary.iteration.max(0) as u64,
                    tokens: outcome.summary.tokens.max(0) as u64,
                    usd_approx: outcome.summary.usd_approx,
                    usd_real: outcome.summary.usd_real,
                },
                error: outcome.error.clone(),
            });
            run.pending_input = None;
            run.input_tx = None;
            run.updated_at = Utc::now();
        }
    }
}

fn map_run_state(state: LoopRunState) -> RunState {
    match state {
        LoopRunState::Running => RunState::Running,
        LoopRunState::WaitingInput => RunState::WaitingInput,
        LoopRunState::Stopping => RunState::Stopping,
        LoopRunState::Finished => RunState::Finished,
        LoopRunState::Failed => RunState::Failed,
        LoopRunState::Stopped => RunState::Stopped,
    }
}

fn map_summary(summary: &LoopSummary) -> RunSummary {
    RunSummary {
        iteration: summary.iteration.max(0) as u64,
        tokens: summary.tokens.max(0) as u64,
        usd_approx: summary.usd_approx,
        usd_real: summary.usd_real,
    }
}

fn failed_outcome(error: impl Into<String>) -> RunOutcome {
    RunOutcome {
        state: LoopRunState::Failed,
        summary: LoopSummary {
            iteration: 0,
            tokens: 0,
            usd_approx: 0.0,
            usd_real: None,
        },
        error: Some(error.into()),
    }
}

fn stopped_outcome() -> RunOutcome {
    RunOutcome {
        state: LoopRunState::Stopped,
        summary: LoopSummary {
            iteration: 0,
            tokens: 0,
            usd_approx: 0.0,
            usd_real: None,
        },
        error: None,
    }
}

fn source_with_agent_config(
    settings: &ServerSettings,
    config_path: &std::path::Path,
) -> serde_json::Value {
    let mut source = settings.source.clone();
    if let Some(agent) = source
        .as_object_mut()
        .and_then(|root| root.get_mut("agent"))
        .and_then(|agent| agent.as_object_mut())
    {
        agent.insert(
            "config".to_string(),
            serde_json::Value::String(config_path.to_string_lossy().into_owned()),
        );
    }
    source
}
