//! REST transport for the harness HTTPS server.
//!
//! `HarnessGateway` owns base URL normalization, all API methods and the
//! per-request 10-second timeout race. Polling is intentionally left to the
//! UI layer; this module only provides the fetch primitives required by the
//! fixed 10-second polling contract.

use futures::future::Either;
use gloo_net::http::{Request, RequestBuilder, Response};
use gloo_timers::future::TimeoutFuture;
use harness_api::{
    HealthResponse, RunState, RunStatusResponse, StartRunRequest, StartRunResponse,
    StopRunRequest, SubmitInputRequest, SubmitInputResponse,
};
#[cfg(feature = "fs-panel")]
use harness_api::{
    ConfigDefaultResponse, ConfigResponse, FsListResponse, FsMoveResponse, FsMkdirResponse,
    FsReadResponse, FsRemoveResponse, FsWriteRequest, FsWriteResponse, PutConfigRequest,
    PutConfigResponse,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// Fixed polling interval required by the client-server contract.
pub const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Upper bound for a single fetch request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Error type returned by all gateway operations.
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayError {
    /// Transport-level error (network, TLS, CORS, etc.).
    Network(String),
    /// The request did not finish before the 10-second timeout.
    Timeout,
    /// Server returned a non-2xx status.
    Http {
        status: u16,
        code: String,
        message: String,
    },
    /// Response body could not be decoded as JSON.
    Decode(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GatewayError::Network(msg) => write!(f, "network error: {msg}"),
            GatewayError::Timeout => write!(f, "request timed out"),
            GatewayError::Http {
                status,
                code,
                message,
            } => write!(f, "HTTP {status} {code}: {message}"),
            GatewayError::Decode(msg) => write!(f, "decode error: {msg}"),
        }
    }
}

impl std::error::Error for GatewayError {}

impl From<gloo_net::Error> for GatewayError {
    fn from(value: gloo_net::Error) -> Self {
        GatewayError::Network(value.to_string())
    }
}

impl From<serde_json::Error> for GatewayError {
    fn from(value: serde_json::Error) -> Self {
        GatewayError::Decode(value.to_string())
    }
}

/// Response of `POST /api/v1/run/stop`.
///
/// `harness-api` intentionally only declares `StopRunRequest`; the response is
/// a tiny `runState` envelope, so it is defined locally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopRunResponse {
    /// State right after a stop request.
    pub run_state: RunState,
}

/// What the polling layer should do after receiving a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollAction {
    /// Keep polling.
    Continue,
    /// Open the input modal for `pendingInput`.
    ShowInput,
    /// Show the terminal result and stop polling.
    ShowResult,
}

/// Classifies a polling response into the next UI action.
pub fn classify_status(status: &RunStatusResponse) -> PollAction {
    if status.pending_input.is_some() {
        PollAction::ShowInput
    } else if status.result.is_some() {
        PollAction::ShowResult
    } else {
        PollAction::Continue
    }
}

/// Returns whether a run state is terminal.
pub fn is_terminal_run_state(state: RunState) -> bool {
    matches!(
        state,
        RunState::Finished | RunState::Failed | RunState::Stopped
    )
}

/// REST transport to `harness.exe --serve`.
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessGateway {
    base_url: String,
    auth_token: Option<String>,
}

impl HarnessGateway {
    /// Creates a gateway with a normalized base URL and an optional token.
    pub fn new(base_url: impl Into<String>, auth_token: Option<String>) -> Self {
        Self {
            base_url: Self::normalize_base_url(&base_url.into()),
            auth_token,
        }
    }

    /// Removes surrounding whitespace and trailing slashes from a base URL.
    pub fn normalize_base_url(base_url: &str) -> String {
        base_url.trim().trim_end_matches('/').to_string()
    }

    /// Joins a relative `run_url` to a base URL without re-escaping it.
    pub fn join_url(base_url: &str, run_url: &str) -> String {
        let base = Self::normalize_base_url(base_url);
        if run_url.is_empty() {
            base
        } else if run_url.starts_with('/') {
            format!("{base}{run_url}")
        } else {
            format!("{base}/{run_url}")
        }
    }

    /// Normalized base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Current auth token, if any.
    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    /// `GET /api/v1/health` (unauthenticated).
    pub async fn health(&self) -> Result<HealthResponse, GatewayError> {
        let url = self.join("/api/v1/health");
        let resp = send(Request::get(&url)).await?;
        parse_json(resp).await
    }

    /// `POST /api/v1/run`.
    pub async fn start_run(
        &self,
        prompt: impl Into<String>,
    ) -> Result<StartRunResponse, GatewayError> {
        let body = StartRunRequest {
            prompt: prompt.into(),
        };
        self.post_json("/api/v1/run", &body).await
    }

    /// `GET {run_url}` polling request.
    pub async fn get_run_status(&self, run_url: &str) -> Result<RunStatusResponse, GatewayError> {
        let url = self.join(run_url);
        let req = self.apply_auth(Request::get(&url));
        let resp = send(req).await?;
        parse_json(resp).await
    }

    /// `POST /api/v1/run/stop`.
    pub async fn stop_run(
        &self,
        reason: impl Into<String>,
    ) -> Result<StopRunResponse, GatewayError> {
        let body = StopRunRequest {
            reason: Some(reason.into()),
        };
        self.post_json("/api/v1/run/stop", &body).await
    }

    /// `POST /api/v1/run/input`.
    pub async fn submit_input(
        &self,
        request_id: impl Into<String>,
        answer: impl Into<String>,
    ) -> Result<SubmitInputResponse, GatewayError> {
        let body = SubmitInputRequest {
            request_id: request_id.into(),
            answer: answer.into(),
        };
        self.post_json("/api/v1/run/input", &body).await
    }

    fn join(&self, path_or_url: &str) -> String {
        Self::join_url(&self.base_url, path_or_url)
    }

    fn apply_auth(&self, req: RequestBuilder) -> RequestBuilder {
        match &self.auth_token {
            Some(token) => req.header("Authorization", &format!("Bearer {token}")),
            None => req,
        }
    }

    #[cfg(feature = "fs-panel")]
    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, GatewayError> {
        let req = self.apply_auth(Request::get(&self.join(path)));
        let resp = send(req).await?;
        parse_json(resp).await
    }

    #[cfg(feature = "fs-panel")]
    async fn get_json_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, GatewayError> {
        let req = Request::get(&self.join(path)).query(query.iter().copied());
        let req = self.apply_auth(req);
        let resp = send(req).await?;
        parse_json(resp).await
    }

    async fn post_json<T: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, GatewayError> {
        let builder = Request::post(&self.join(path));
        let builder = self.apply_auth(builder);
        let req = builder.json(body).map_err(GatewayError::from)?;
        let resp = send_request(req).await?;
        parse_json(resp).await
    }

    #[cfg(feature = "fs-panel")]
    async fn post_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, GatewayError> {
        let req = Request::post(&self.join(path)).query(query.iter().copied());
        let req = self.apply_auth(req);
        let resp = send(req).await?;
        parse_json(resp).await
    }

    #[cfg(feature = "fs-panel")]
    async fn put_json<T: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<R, GatewayError> {
        let builder = Request::put(&self.join(path));
        let builder = self.apply_auth(builder);
        let req = builder.json(body).map_err(GatewayError::from)?;
        let resp = send_request(req).await?;
        parse_json(resp).await
    }

    #[cfg(feature = "fs-panel")]
    async fn put_json_with_query<T: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
        body: &T,
    ) -> Result<R, GatewayError> {
        let builder = Request::put(&self.join(path)).query(query.iter().copied());
        let builder = self.apply_auth(builder);
        let req = builder.json(body).map_err(GatewayError::from)?;
        let resp = send_request(req).await?;
        parse_json(resp).await
    }

    #[cfg(feature = "fs-panel")]
    async fn delete_with_query<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, GatewayError> {
        let req = Request::delete(&self.join(path)).query(query.iter().copied());
        let req = self.apply_auth(req);
        let resp = send(req).await?;
        parse_json(resp).await
    }
}

#[cfg(feature = "fs-panel")]
impl HarnessGateway {
    /// `GET /api/v1/config`.
    pub async fn get_config(&self) -> Result<ConfigResponse, GatewayError> {
        self.get_json("/api/v1/config").await
    }

    /// `PUT /api/v1/config`.
    pub async fn put_config(&self, text: impl Into<String>) -> Result<PutConfigResponse, GatewayError> {
        let body = PutConfigRequest { text: text.into() };
        self.put_json("/api/v1/config", &body).await
    }

    /// `POST /api/v1/config/default`.
    pub async fn config_default(&self) -> Result<ConfigDefaultResponse, GatewayError> {
        let req = self.apply_auth(Request::post(&self.join("/api/v1/config/default")));
        let resp = send(req).await?;
        parse_json(resp).await
    }

    /// `GET /api/v1/fs/list`.
    pub async fn fs_list(&self, path: &str) -> Result<FsListResponse, GatewayError> {
        self.get_json_with_query("/api/v1/fs/list", &[("path", path)]).await
    }

    /// `GET /api/v1/fs/read`.
    pub async fn fs_read(&self, path: &str) -> Result<FsReadResponse, GatewayError> {
        self.get_json_with_query("/api/v1/fs/read", &[("path", path)]).await
    }

    /// `PUT /api/v1/fs/write`.
    pub async fn fs_write(
        &self,
        path: &str,
        text: impl Into<String>,
    ) -> Result<FsWriteResponse, GatewayError> {
        let body = FsWriteRequest { text: text.into() };
        self.put_json_with_query("/api/v1/fs/write", &[("path", path)], &body)
            .await
    }

    /// `POST /api/v1/fs/mkdir`.
    pub async fn fs_mkdir(&self, path: &str) -> Result<FsMkdirResponse, GatewayError> {
        self.post_with_query("/api/v1/fs/mkdir", &[("path", path)]).await
    }

    /// `DELETE /api/v1/fs/remove`.
    pub async fn fs_remove(&self, path: &str) -> Result<FsRemoveResponse, GatewayError> {
        self.delete_with_query("/api/v1/fs/remove", &[("path", path)]).await
    }

    /// `POST /api/v1/fs/move`.
    pub async fn fs_move(&self, src: &str, dst: &str) -> Result<FsMoveResponse, GatewayError> {
        self.post_with_query("/api/v1/fs/move", &[("src", src), ("dst", dst)])
            .await
    }
}

/// Sends a request, racing it against a 10-second timeout.
async fn send(req: RequestBuilder) -> Result<Response, GatewayError> {
    let request = req.send();
    futures::pin_mut!(request);

    let timeout = TimeoutFuture::new(REQUEST_TIMEOUT.as_millis() as u32);
    futures::pin_mut!(timeout);

    match futures::future::select(request, timeout).await {
        Either::Left((result, _)) => result.map_err(GatewayError::from),
        Either::Right((_, _)) => Err(GatewayError::Timeout),
    }
}

/// Sends a built request, racing it against a 10-second timeout.
async fn send_request(req: Request) -> Result<Response, GatewayError> {
    let request = req.send();
    futures::pin_mut!(request);

    let timeout = TimeoutFuture::new(REQUEST_TIMEOUT.as_millis() as u32);
    futures::pin_mut!(timeout);

    match futures::future::select(request, timeout).await {
        Either::Left((result, _)) => result.map_err(GatewayError::from),
        Either::Right((_, _)) => Err(GatewayError::Timeout),
    }
}

/// Parses a JSON response body or turns a non-2xx response into an error.
async fn parse_json<T: DeserializeOwned>(resp: Response) -> Result<T, GatewayError> {
    let status = resp.status();
    let text = resp.text().await.map_err(GatewayError::from)?;

    if (200..300).contains(&status) {
        serde_json::from_str(&text).map_err(GatewayError::from)
    } else {
        let (code, message) = extract_error(&text)
            .unwrap_or_else(|| (format!("HTTP_{status}"), format!("HTTP {status}")));
        Err(GatewayError::Http {
            status,
            code,
            message,
        })
    }
}

fn extract_error(text: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let error = value.get("error")?;
    let code = error.get("code")?.as_str()?.to_string();
    let message = error.get("message")?.as_str()?.to_string();
    Some((code, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_api::PendingInputKind;

    #[test]
    fn join_url_concatenates_without_escaping() {
        assert_eq!(
            HarnessGateway::join_url(
                "https://localhost:4067/",
                "/api/v1/run/current"
            ),
            "https://localhost:4067/api/v1/run/current"
        );
        assert_eq!(
            HarnessGateway::join_url("https://localhost:4067", "api/v1/run/current"),
            "https://localhost:4067/api/v1/run/current"
        );
        assert_eq!(
            HarnessGateway::join_url("https://localhost:4067///", "/api/v1/run/current"),
            "https://localhost:4067/api/v1/run/current"
        );
    }

    #[test]
    fn run_status_with_pending_input_sensitive_defaults_to_false() {
        let status: RunStatusResponse = serde_json::from_str(
            r#"{
                "runState": "waitingInput",
                "pendingInput": {
                    "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
                    "kind": "text",
                    "prompt": "Enter API key:",
                    "timeoutAt": "2026-01-01T12:15:00Z"
                }
            }"#,
        )
        .unwrap();

        assert_eq!(status.run_state, RunState::WaitingInput);
        assert!(!status.pending_input.unwrap().sensitive);
    }

    #[test]
    fn run_status_with_pending_input_sensitive_true() {
        let status: RunStatusResponse = serde_json::from_str(
            r#"{
                "runState": "waitingInput",
                "pendingInput": {
                    "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
                    "kind": "text",
                    "prompt": "Enter API key:",
                    "timeoutAt": "2026-01-01T12:15:00Z",
                    "sensitive": true
                }
            }"#,
        )
        .unwrap();

        let pending = status.pending_input.unwrap();
        assert_eq!(pending.kind, PendingInputKind::Text);
        assert!(pending.sensitive);
    }

    #[test]
    fn polling_state_transitions() {
        let idle: RunStatusResponse = serde_json::from_str(r#"{"runState":"idle"}"#).unwrap();
        assert!(matches!(classify_status(&idle), PollAction::Continue));

        let running: RunStatusResponse =
            serde_json::from_str(r#"{"runState":"running"}"#).unwrap();
        assert!(matches!(classify_status(&running), PollAction::Continue));

        let waiting: RunStatusResponse = serde_json::from_str(
            r#"{
                "runState": "waitingInput",
                "pendingInput": {
                    "requestId": "req_01J9Z8X7V6T5R4P3N2M1Q",
                    "kind": "text",
                    "prompt": "Choose agent",
                    "timeoutAt": "2026-01-01T12:15:00Z"
                }
            }"#,
        )
        .unwrap();
        assert!(matches!(classify_status(&waiting), PollAction::ShowInput));

        let finished: RunStatusResponse = serde_json::from_str(
            r#"{
                "runState": "finished",
                "result": {
                    "ok": true,
                    "runState": "finished",
                    "stats": {
                        "iteration": 3,
                        "tokens": 100,
                        "usdApprox": 0.1
                    },
                    "error": null
                }
            }"#,
        )
        .unwrap();
        assert!(matches!(classify_status(&finished), PollAction::ShowResult));
        assert!(is_terminal_run_state(RunState::Finished));
    }
}
