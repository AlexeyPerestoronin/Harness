//! Agent loop: session folder, limit checks, iteration, dump.
//!
//! Behaviour matches Python `tools/harness/agent_loop.py`: increment iteration,
//! log the header, enforce token / USD / iteration limits, call `agent.iteration`,
//! dump, stop or continue. `--dump` restores state into a **new** log folder.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;
use conf::{AgentKind, AgentSettings, ToolsSettings};
use interactions::{Interaction, InteractionKind, InteractionRequest, OutputType};
use tokio_util::sync::CancellationToken;
use tools::{ToolRegistry, ToolsContext};

use crate::agents::{Agent, AlibabaAgent, ClaudeAgent, DeepSeekAgent, GoogleAgent, XaiAgent};

/// Terminal and transient states of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    WaitingInput,
    Stopping,
    Finished,
    Failed,
    Stopped,
}

/// Snapshot of the run counters.
#[derive(Debug, Clone)]
pub struct RunSummary {
    pub iteration: i64,
    pub tokens: i64,
    pub usd_approx: f64,
    pub usd_real: Option<f64>,
}

/// Final result of [`AgentLoop::run`].
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub state: RunState,
    pub summary: RunSummary,
    pub error: Option<String>,
}

/// Observer notified on state changes and at least once per iteration.
#[async_trait::async_trait]
pub trait RunObserver: Send + Sync {
    async fn on_state(&self, state: RunState);
    async fn on_summary(&self, summary: RunSummary);
}

/// Drive one harness session.
pub struct AgentLoop {
    safe_mode: bool,
    interaction: Arc<dyn Interaction>,
    iteration: i64,
    iteration_limit: i64,
    agent: Box<dyn Agent>,
    agent_config: PathBuf,
    dump_settings: serde_json::Value,
}

impl AgentLoop {
    /// Create the tool registry and the vendor agent.
    ///
    /// `session_dir` is used as the tool `temp-dir` for patches.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cwd: PathBuf,
        _log_folder: String,
        agent: AgentSettings,
        tools: ToolsSettings,
        interaction: Arc<dyn Interaction>,
        session_dir: PathBuf,
    ) -> Result<Self, String> {
        let registry = Arc::new(ToolRegistry::build(
            &tools,
            ToolsContext {
                cwd,
                temp_dir: session_dir,
                interaction: interaction.clone(),
            },
        )?);
        let boxed_agent: Box<dyn Agent> = match &agent.kind {
            AgentKind::Xai { .. } => {
                Box::new(XaiAgent::new(registry, interaction.clone(), &agent)?)
            }
            AgentKind::Google { .. } => {
                Box::new(GoogleAgent::new(registry, interaction.clone(), &agent)?)
            }
            AgentKind::Claude { .. } => {
                Box::new(ClaudeAgent::new(registry, interaction.clone(), &agent)?)
            }
            AgentKind::DeepSeek { .. } => Box::new(DeepSeekAgent::new(
                registry,
                interaction.clone(),
                &agent,
            )?),
            AgentKind::Alibaba { .. } => Box::new(AlibabaAgent::new(
                registry,
                interaction.clone(),
                &agent,
            )?),
        };
        Ok(Self {
            safe_mode: agent.mode.is_safe(),
            interaction,
            iteration: 0,
            iteration_limit: agent.iteration_limit,
            agent: boxed_agent,
            agent_config: agent.config_path.clone(),
            dump_settings: serde_json::Value::Null,
        })
    }

    /// Attach the conf JSON that [`crate::dump_settings::embed`] writes into dumps.
    pub fn with_dump_settings(mut self, source: serde_json::Value) -> Self {
        self.dump_settings = source;
        self
    }

    /// Run the loop. `source_dump` restores history; a new dump is still written to `dump_file`.
    pub async fn run(
        &mut self,
        mut prompt: Option<String>,
        dump_file: &Path,
        source_dump: Option<&Path>,
        cancel: CancellationToken,
        observer: Option<Arc<dyn RunObserver>>,
    ) -> Result<RunOutcome, String> {
        if let Some(src) = source_dump {
            if let Err(e) = self.agent.reload_session(src) {
                return Ok(self.failed(format!(
                    "cannot reload dump '{}': {e}",
                    src.display()
                )));
            }
            prompt = None;
        }
        self.agent.set_cancel_token(cancel.clone());
        self.notify_state(&observer, RunState::Running).await;

        self.log_line("# Agent-loop session:").await;
        self.log_line(&format!(
            "- agent: {} {}",
            self.agent.vendor(),
            self.agent.model()
        ))
        .await;
        self.log_line(&format!("- agent config: {}", self.agent_config.display()))
            .await;
        self.log_line(&format!("- iteration limit: {}", self.iteration_limit))
            .await;
        self.log_line(&format!("- tokens limit: {}", self.agent.tokens_limit()))
            .await;
        self.log_line(&format!("- usd limit: {}", self.agent.limit_usd()))
            .await;
        self.log_line(&format!(
            "- start time: {}",
            Local::now().format("%Y-%m-%d %H:%M")
        ))
        .await;
        if let Some(src) = source_dump {
            self.log_line(&format!("- continue from dump: {}", src.display()))
                .await;
            self.log_line("- settings: from dump").await;
        }
        self.log_line("").await;
        self.log_line("# Iterations:").await;

        let start_time = Instant::now();
        loop {
            if cancel.is_cancelled() {
                return Ok(self.stopped());
            }

            self.iteration += 1;
            let elapsed = start_time.elapsed().as_secs();
            self.log_line("").await;
            self.log_line(&iteration_header(
                self.iteration,
                elapsed,
                self.agent.consumed_tokens(),
                self.agent.consumed_usd(),
                self.agent.consumed_usd_real(),
            ))
            .await;
            self.notify_summary(&observer, self.summary()).await;

            if !self
                .check_limit(
                    "tokens",
                    self.agent.tokens_limit() as f64,
                    self.agent.consumed_tokens() as f64,
                )
                .await?
            {
                return Ok(self.failed("interrupt loop: token limit exceed"));
            }
            if !self
                .check_limit("USD", self.agent.limit_usd(), self.agent.consumed_usd())
                .await?
            {
                return Ok(self.failed("interrupt loop: USD limit exceed"));
            }
            if !self
                .check_limit(
                    "iteration",
                    self.iteration_limit as f64,
                    self.iteration as f64,
                )
                .await?
            {
                return Ok(self.failed("interrupt loop: iteration limit exceed"));
            }

            if cancel.is_cancelled() {
                return Ok(self.stopped());
            }

            let stop = match self.agent.iteration(prompt.as_deref()).await {
                Ok(stop) => stop,
                Err(e) => return Ok(self.failed(e)),
            };
            if let Err(e) = self.agent.dump_session(dump_file) {
                return Ok(self.failed(e));
            }
            if let Err(e) = crate::dump_settings::embed(dump_file, &self.dump_settings) {
                return Ok(self.failed(e));
            }
            if stop {
                self.log_line("# Session results:").await;
                self.agent.finish().await;
                let secs = start_time.elapsed().as_secs();
                let formatted = format!(
                    "{}:{:02}:{:02}",
                    secs / 3600,
                    (secs % 3600) / 60,
                    secs % 60
                );
                self.log_line(&format!("- Execution time: {formatted}")).await;
                self.log_line("").await;
                return Ok(self.finished());
            }
            prompt = None;
        }
    }

    fn summary(&self) -> RunSummary {
        RunSummary {
            iteration: self.iteration,
            tokens: self.agent.consumed_tokens(),
            usd_approx: self.agent.consumed_usd(),
            usd_real: self.agent.consumed_usd_real(),
        }
    }

    fn failed(&self, error: impl Into<String>) -> RunOutcome {
        RunOutcome {
            state: RunState::Failed,
            summary: self.summary(),
            error: Some(error.into()),
        }
    }

    fn stopped(&self) -> RunOutcome {
        RunOutcome {
            state: RunState::Stopped,
            summary: self.summary(),
            error: None,
        }
    }

    fn finished(&self) -> RunOutcome {
        RunOutcome {
            state: RunState::Finished,
            summary: self.summary(),
            error: None,
        }
    }

    async fn check_limit(&self, kind: &str, limit: f64, consumed: f64) -> Result<bool, String> {
        if (limit - -1.0).abs() > f64::EPSILON && consumed > limit {
            let message = format!("⚠️ {kind} limit exceed ({consumed} > {limit})");
            self.log_line(&message).await;
            if self.safe_mode {
                let answer = self
                    .ask_confirm(&format!("{message} → resume execution? [y/N]: "))
                    .await?;
                return Ok(is_yes(&answer));
            }
        }
        Ok(true)
    }

    async fn ask_confirm(&self, prompt: &str) -> Result<String, String> {
        let request = InteractionRequest {
            kind: InteractionKind::Confirm,
            prompt: prompt.to_string(),
            sensitive: false,
        };
        self.interaction
            .input(&request, true)
            .await
            .map_err(|e| format!("cannot ask user → {e}"))
    }

    async fn log_line(&self, message: &str) {
        let _ = self
            .interaction
            .output(message, true, OutputType::NewLine)
            .await;
    }

    async fn notify_state(&self, observer: &Option<Arc<dyn RunObserver>>, state: RunState) {
        if let Some(observer) = observer {
            let _ = observer.on_state(state).await;
        }
    }

    async fn notify_summary(&self, observer: &Option<Arc<dyn RunObserver>>, summary: RunSummary) {
        if let Some(observer) = observer {
            let _ = observer.on_summary(summary).await;
        }
    }
}

/// Build the iteration header block logged before each model call.
fn iteration_header(
    iteration: i64,
    elapsed_secs: u64,
    tokens: i64,
    usd: f64,
    real_usd: Option<f64>,
) -> String {
    format!(
        "## Iteration №{}\nUsage:\n- time = {}\n- tokens = {}\n- USD approximated = {}\n- USD real = {}",
        iteration,
        format_elapsed(elapsed_secs),
        tokens,
        crate::agents::format_usd(usd),
        crate::agents::format_real_cost(real_usd)
    )
}

fn format_elapsed(secs: u64) -> String {
    format!("{} min {} sec", secs / 60, secs % 60)
}

fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "y" | "yes" | "д" | "да"
    )
}

/// `{cwd}/{log-folder}/{YYYY-MM-DD HH-MM}_{vendor}_{model}/`
pub fn session_dir(
    cwd: &Path,
    log_folder: &str,
    vendor: &str,
    model: &str,
) -> Result<PathBuf, String> {
    let stamp = Local::now().format("%Y-%m-%d %H-%M");
    let folder = format!("{}_{}_{}", stamp, vendor, model);
    let log_root = tools::fs::resolve_in_root(cwd, log_folder)
        .map_err(|e| format!("invalid log-folder '{}': {e}", log_folder))?;
    let dir = log_root.join(folder);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create log dir: {e}"))?;
    Ok(dir)
}
