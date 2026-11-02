//! Shared REST DTOs for the harness server and web client.
//!
//! The crate intentionally has no axum/leptos/auth dependency: it only uses
//! serde so it can be linked into the server and into WASM clients alike.
//! All JSON field names follow the camelCase REST contract.

use serde::{Deserialize, Serialize};

/// Lifecycle state of the single active run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    /// No run has been started yet.
    Idle,
    /// Agent loop is executing.
    Running,
    /// Agent loop is waiting for user input.
    WaitingInput,
    /// Stop has been requested.
    Stopping,
    /// Run completed successfully.
    Finished,
    /// Run failed.
    Failed,
    /// Run was stopped by the user.
    Stopped,
}

/// Configuration validity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConfigState {
    /// Configuration loaded and valid.
    Ok,
    /// Configuration file does not exist.
    Missing,
    /// Configuration file exists but is invalid.
    Invalid,
}

/// Kind of a pending user input request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PendingInputKind {
    /// Arbitrary text answer.
    Text,
    /// Confirmation answer (`y`/`n`).
    Confirm,
}

/// Machine-readable error body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// Stable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

/// JSON envelope for an error response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    /// Nested error object required by the REST contract.
    pub error: ErrorBody,
}

/// `GET /api/v1/health` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Health status, always `"ok"`.
    pub status: String,
    /// Server version.
    pub version: String,
    /// ISO-8601 server time.
    pub server_time: String,
    /// Whether protected routes require a bearer token.
    pub auth_required: bool,
    /// Current run state.
    pub run_state: RunState,
    /// Current configuration state.
    pub config_state: ConfigState,
}

/// `POST /api/v1/run` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRunRequest {
    /// User prompt for the run.
    pub prompt: String,
}

/// `POST /api/v1/run` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRunResponse {
    /// Run state right after start.
    pub run_state: RunState,
    /// Relative URL hook for polling the current run.
    pub run_url: String,
}

/// Aggregated run counters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// Iteration number.
    pub iteration: u64,
    /// Consumed tokens.
    pub tokens: u64,
    /// Approximated USD cost.
    pub usd_approx: f64,
    /// Real USD cost when reported by the vendor.
    pub usd_real: Option<f64>,
}

/// Active request for user input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInput {
    /// Unique request id with `req_` prefix.
    pub request_id: String,
    /// Expected answer kind.
    pub kind: PendingInputKind,
    /// Prompt shown to the user.
    pub prompt: String,
    /// ISO-8601 deadline for the answer.
    pub timeout_at: String,
    /// Explicit flag for secret input (e.g. API key).
    #[serde(default)]
    pub sensitive: bool,
}

/// Run statistics stored in the terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStats {
    /// Iteration number.
    pub iteration: u64,
    /// Consumed tokens.
    pub tokens: u64,
    /// Approximated USD cost.
    pub usd_approx: f64,
    /// Real USD cost when reported by the vendor.
    pub usd_real: Option<f64>,
}

/// Terminal result of a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    /// Whether the run completed successfully.
    pub ok: bool,
    /// Terminal run state.
    pub run_state: RunState,
    /// Final run statistics.
    pub stats: RunStats,
    /// Error message for failed runs.
    pub error: Option<String>,
}

/// `GET /api/v1/run/current` response.
///
/// All fields except `run_state` are optional and omitted from JSON when
/// absent, matching the polling contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatusResponse {
    /// Current/last run state.
    pub run_state: RunState,
    /// ISO-8601 start time of the current run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// ISO-8601 last update time of the current run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    /// Run counters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<RunSummary>,
    /// Active user-input request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_input: Option<PendingInput>,
    /// Terminal result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<RunResult>,
}

/// `POST /api/v1/run/stop` request.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopRunRequest {
    /// Optional stop reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `POST /api/v1/run/input` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitInputRequest {
    /// Id of the pending input being answered.
    pub request_id: String,
    /// User answer.
    pub answer: String,
}

/// `POST /api/v1/run/input` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitInputResponse {
    /// Always `true` when the answer was accepted.
    pub accepted: bool,
}

/// `GET /api/v1/config` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// Absolute path of `harness.json5`.
    pub path: String,
    /// Whether the file exists.
    pub exists: bool,
    /// Config text (redacted when valid; `null` when unparseable).
    pub text: Option<String>,
    /// Whether the current config is valid.
    pub valid: bool,
    /// Validation error message when config is invalid.
    pub validation_error: Option<String>,
}

/// `PUT /api/v1/config` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutConfigRequest {
    /// New config text.
    pub text: String,
}

/// `PUT /api/v1/config` response (success or failure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PutConfigResponse {
    /// Whether the config was saved.
    pub ok: bool,
    /// Saved config path (on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Validation flag (on success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid: Option<bool>,
    /// Validation error (on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

/// `PUT /api/v1/config` error response for `CONFIG_INVALID`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInvalidResponse {
    /// Always `false` for a validation error.
    pub ok: bool,
    /// Validation error details.
    pub error: ErrorBody,
}

/// `POST /api/v1/config/default` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDefaultResponse {
    /// Absolute path of `harness.json5`.
    pub path: String,
    /// Whether the file exists after the call.
    pub exists: bool,
    /// Config text.
    pub text: String,
    /// Whether the config is valid.
    pub valid: bool,
}

/// `GET /api/v1/fs/list` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsListResponse {
    /// Requested relative path.
    pub path: String,
    /// Directory entries.
    pub entries: Vec<FsEntry>,
}

/// File system entry in an FS list response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    /// Entry name.
    pub name: String,
    /// Entry kind (`file` or `dir`).
    pub kind: String,
    /// File size in bytes (or `0` for directories).
    pub size: u64,
}

/// `GET /api/v1/fs/read` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsReadResponse {
    /// Requested relative path.
    pub path: String,
    /// UTF-8 file content.
    pub text: String,
}

/// `PUT /api/v1/fs/write` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteRequest {
    /// New UTF-8 file content.
    pub text: String,
}

/// `PUT /api/v1/fs/write` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteResponse {
    /// Whether the file was written.
    pub ok: bool,
    /// Written relative path.
    pub path: String,
}

/// `POST /api/v1/fs/mkdir` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsMkdirResponse {
    /// Whether the directory was created.
    pub ok: bool,
    /// Created relative path.
    pub path: String,
}

/// `DELETE /api/v1/fs/remove` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsRemoveResponse {
    /// Whether the entry was removed.
    pub ok: bool,
    /// Removed relative path.
    pub path: String,
}

/// `POST /api/v1/fs/move` response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsMoveResponse {
    /// Whether the entry was moved.
    pub ok: bool,
    /// Source relative path.
    pub src: String,
    /// Destination relative path.
    pub dst: String,
}

