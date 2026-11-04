//! Server state shared by the HTTP handlers and the spawned agent-loop task.
//!
//! All locks are [`tokio::sync`] primitives so guards can be held across
//! `.await` points. The server owns exactly one [`ActiveRun`]; its lifecycle
//! is managed by [`crate::server::handlers`] and
//! [`crate::server::run`].

use std::path::PathBuf;
use std::time::{Duration, Instant};

use auth::AuthToken;
use chrono::{DateTime, Utc};
use conf::{ServerConfig, ServerSettings};
use harness_api::{ConfigState, PendingInput, RunResult, RunState, RunSummary};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

/// Shared server state.
pub struct ServerState {
    /// Loaded configuration snapshot.
    pub config: RwLock<LoadedServerConfig>,
    /// The single active / terminal run. `None` before the first `POST /run`.
    pub active: Mutex<Option<ActiveRun>>,
    /// Bearer token used by the protected router.
    pub auth_token: AuthToken,
    /// Idle timeout after which the server exits.
    pub live_timeout: Duration,
    /// Deadline for the idle timer. Mutating API calls reset it.
    pub live_deadline: Mutex<Instant>,
}

impl ServerState {
    /// Reset the live-timeout deadline to `now + live_timeout`.
    pub async fn touch_live_deadline(&self) {
        *self.live_deadline.lock().await = Instant::now() + self.live_timeout;
    }
}

/// Configuration snapshot stored in [`ServerState`].
#[derive(Debug, Clone)]
pub struct LoadedServerConfig {
    /// Absolute path of `harness.json5` used by the server.
    pub path: PathBuf,
    /// Effective CWD the server was started with (and validates configs against).
    pub cwd: PathBuf,
    /// Coarse configuration state: `ok` | `missing` | `invalid`.
    pub config_state: ConfigState,
    /// Fully validated server settings when `config_state` is `ok`.
    pub settings: Option<ServerSettings>,
    /// Effective server block after CLI overrides.
    pub server: ServerConfig,
}

/// The single run slot managed by the server.
pub struct ActiveRun {
    /// Internal run identifier (`run_` + ULID).
    pub run_id: String,
    /// UTC start time of the run.
    pub started_at: DateTime<Utc>,
    /// UTC last update time of the run.
    pub updated_at: DateTime<Utc>,
    /// Current lifecycle state.
    pub state: RunState,
    /// Latest counters reported by the agent loop.
    pub summary: RunSummary,
    /// Active user-input request, present while `state` is `waiting_input`.
    pub pending_input: Option<PendingInput>,
    /// Terminal result, present after the run reached `finished`/`failed`/`stopped`.
    pub result: Option<RunResult>,
    /// Cancellation token owned by this run.
    pub cancel: CancellationToken,
    /// One-shot sender used by [`crate::server::web_interaction::WebInteraction`].
    pub input_tx: Option<oneshot::Sender<UserResponse>>,
}

/// Answer delivered by the HTTP input endpoint to a waiting interaction.
#[derive(Debug)]
pub enum UserResponse {
    /// Text answer (for both `text` and `confirm` kinds).
    Answer(String),
}

/// `true` for states that occupy the single active-run slot.
pub fn is_active(state: RunState) -> bool {
    matches!(
        state,
        RunState::Running | RunState::WaitingInput | RunState::Stopping
    )
}

/// `true` for terminal run states.
pub fn is_terminal(state: RunState) -> bool {
    matches!(
        state,
        RunState::Finished | RunState::Failed | RunState::Stopped
    )
}

/// Create an empty run summary used before the first agent-loop report.
pub fn empty_summary() -> RunSummary {
    RunSummary {
        iteration: 0,
        tokens: 0,
        usd_approx: 0.0,
        usd_real: None,
    }
}

