//! Agent loop: session folder, limit checks, iteration, dump.
//!
//! Behaviour matches Python `tools/harness/agent_loop.py`: increment iteration,
//! log the header, enforce token / USD / iteration limits, call `agent.iteration`,
//! dump, stop or continue. `--dump` restores state into a **new** log folder.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::Local;
use conf::{AgentKind, Settings};
use logger::{DoubleLogger, Logger};
use tools::{ToolRegistry, ToolsContext};

use crate::agents::{Agent, ClaudeAgent, GoogleAgent, XaiAgent};

/// Drive one harness session.
pub struct AgentLoop {
    safe_mode: bool,
    logger: Arc<dyn Logger>,
    iteration: i64,
    iteration_limit: i64,
    agent: Box<dyn Agent>,
}

impl AgentLoop {
    /// Create logger, tool registry and the vendor agent.
    pub fn new(settings: &Settings, log_file: &Path, temp_dir: PathBuf) -> Result<Self, String> {
        let logger: Arc<dyn Logger> = Arc::new(
            DoubleLogger::new(log_file).map_err(|e| format!("cannot open log: {e}"))?,
        );
        let registry = Arc::new(ToolRegistry::build(
            &settings.tools,
            ToolsContext {
                cwd: settings.cwd.clone(),
                temp_dir,
                logger: logger.clone(),
            },
        ));
        let agent: Box<dyn Agent> = match &settings.agent.kind {
            AgentKind::Xai { .. } => {
                Box::new(XaiAgent::new(registry, logger.clone(), &settings.agent)?)
            }
            AgentKind::Google { .. } => {
                Box::new(GoogleAgent::new(registry, logger.clone(), &settings.agent)?)
            }
            AgentKind::Claude { .. } => {
                Box::new(ClaudeAgent::new(registry, logger.clone(), &settings.agent)?)
            }
        };
        Ok(Self {
            safe_mode: settings.agent.mode.is_safe(),
            logger,
            iteration: 0,
            iteration_limit: settings.agent.iteration_limit,
            agent,
        })
    }

    /// Run the loop. `source_dump` restores history; a new dump is still written to `dump_file`.
    pub fn start(
        &mut self,
        mut prompt: Option<String>,
        dump_file: &Path,
        source_dump: Option<&Path>,
    ) -> Result<(), String> {
        if let Some(src) = source_dump {
            self.agent.reload_session(src)?;
            prompt = None;
        }
        self.logger.log_line("# Agent-loop session:");
        self.logger.log_line(&format!(
            "- agent: {} {}",
            self.agent.vendor(),
            self.agent.model()
        ));
        self.logger
            .log_line(&format!("- iteration limit: {}", self.iteration_limit));
        self.logger
            .log_line(&format!("- tokens limit: {}", self.agent.tokens_limit()));
        self.logger
            .log_line(&format!("- usd limit: {}", self.agent.limit_usd()));
        self.logger.log_line(&format!(
            "- start time: {}",
            Local::now().format("%Y-%m-%d %H:%M")
        ));
        if let Some(src) = source_dump {
            self.logger
                .log_line(&format!("- continue from dump: {}", src.display()));
        }
        self.logger.log_line("");
        self.logger.log_line("# Iterations:");

        let start_time = Instant::now();
        loop {
            self.iteration += 1;
            self.logger.log_line("");
            self.logger.log_line(&format!(
                "## Iteration №{}: (tokens = {}; usd = {})",
                self.iteration,
                self.agent.consumed_tokens(),
                self.agent.consumed_usd()
            ));

            if !self.check_limit(
                "tokens",
                self.agent.tokens_limit() as f64,
                self.agent.consumed_tokens() as f64,
            ) {
                return Err("interrupt loop: token limit exceed".into());
            }
            if !self.check_limit("USD", self.agent.limit_usd(), self.agent.consumed_usd()) {
                return Err("interrupt loop: USD limit exceed".into());
            }
            if !self.check_limit(
                "iteration",
                self.iteration_limit as f64,
                self.iteration as f64,
            ) {
                return Err("interrupt loop: iteration limit exceed".into());
            }

            let stop = self.agent.iteration(prompt.as_deref())?;
            self.agent.dump_session(dump_file)?;
            if stop {
                self.logger.log_line("# Session results:");
                self.agent.finish();
                break;
            }
            prompt = None;
        }
        let secs = start_time.elapsed().as_secs();
        let formatted = format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60);
        self.logger.log_line(&format!("# Execution time: {formatted}"));
        self.logger.log_line("");
        Ok(())
    }

    fn check_limit(&self, kind: &str, limit: f64, consumed: f64) -> bool {
        if (limit - -1.0).abs() > f64::EPSILON && consumed > limit {
            let message = format!("⚠️ {kind} limit exceed ({consumed} > {limit})");
            self.logger.log_line(&message);
            if self.safe_mode {
                return confirm(&format!("{message} → resume execution? [y/N]: "));
            }
        }
        true
    }
}

fn confirm(prompt: &str) -> bool {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    if io::stdin().read_line(&mut buf).is_err() {
        return false;
    }
    matches!(buf.trim().to_lowercase().as_str(), "y" | "yes")
}

/// `{cwd}/{log-folder}/{YYYY-MM-DD HH-MM}_{vendor}_{model}/`
pub fn session_dir(settings: &Settings) -> Result<PathBuf, String> {
    let stamp = Local::now().format("%Y-%m-%d %H-%M");
    let folder = format!(
        "{}_{}_{}",
        stamp, settings.agent.vendor, settings.agent.name
    );
    let dir = settings.cwd.join(&settings.log_folder).join(folder);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create log dir: {e}"))?;
    Ok(dir)
}
