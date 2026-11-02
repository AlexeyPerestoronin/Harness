//! Axum HTTP handlers and router assembly for the HTTPS server.
//!
//! Only `GET /api/v1/health` is public. The run lifecycle endpoints are
//! protected by one [`auth::require_auth`] middleware layer. CORS is handled
//! by a separate outer middleware so preflight requests never reach auth.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{middleware, Json, Router};
use chrono::{SecondsFormat, Utc};
use harness_api::{
    ConfigState, ErrorBody, ErrorResponse, HealthResponse, PendingInputKind, RunState,
    RunStatusResponse, StartRunRequest, StartRunResponse, StopRunRequest, SubmitInputRequest,
    SubmitInputResponse,
};
use tokio_util::sync::CancellationToken;

use super::run::run_agent_task;
use super::state::{empty_summary, is_active, is_terminal, ActiveRun, ServerState};

/// Error response shaped as `api.md` requires: `{ "error": { "code", "message" } }`.
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
}

impl ApiError {
    /// Build an API error.
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

/// `GET /api/v1/health` (public).
pub async fn health(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<HealthResponse>, ApiError> {
    let config = state.config.read().await;
    let active = state.active.lock().await;
    let run_state = active
        .as_ref()
        .map(|run| run.state)
        .unwrap_or(RunState::Idle);

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        server_time: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        auth_required: true,
        run_state,
        config_state: config.config_state,
    }))
}

/// `POST /api/v1/run` (protected).
pub async fn start_run(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<StartRunRequest>,
) -> Result<Response, ApiError> {
    state.touch_live_deadline().await;

    if request.prompt.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "INVALID_PROMPT",
            "prompt must not be empty",
        ));
    }

    let config_state = state.config.read().await.config_state;
    match config_state {
        ConfigState::Missing => {
            return Err(ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "CONFIG_MISSING",
                "harness.json5 does not exist",
            ))
        }
        ConfigState::Invalid => {
            return Err(ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "CONFIG_INVALID",
                "harness.json5 is invalid",
            ))
        }
        ConfigState::Ok => {}
    }

    let run_id = format!("run_{}", ulid::Ulid::new());
    let cancel = CancellationToken::new();
    let now = Utc::now();

    {
        let mut active = state.active.lock().await;
        if let Some(run) = active.as_ref() {
            if is_active(run.state) {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "RUN_ALREADY_ACTIVE",
                    "a run is already active",
                ));
            }
        }
        *active = Some(ActiveRun {
            run_id: run_id.clone(),
            started_at: now,
            updated_at: now,
            state: RunState::Running,
            summary: empty_summary(),
            pending_input: None,
            result: None,
            cancel: cancel.clone(),
            input_tx: None,
        });
    }

    tokio::spawn(run_agent_task(
        state,
        run_id,
        request.prompt.clone(),
        cancel,
    ));

    Ok((
        StatusCode::ACCEPTED,
        Json(StartRunResponse {
            run_state: RunState::Running,
            run_url: "/api/v1/run/current".to_string(),
        }),
    )
        .into_response())
}

/// `GET /api/v1/run/current` (protected).
pub async fn run_status(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<RunStatusResponse>, ApiError> {
    let active = state.active.lock().await;
    let Some(run) = active.as_ref() else {
        return Ok(Json(RunStatusResponse {
            run_state: RunState::Idle,
            started_at: None,
            updated_at: None,
            summary: None,
            pending_input: None,
            result: None,
        }));
    };

    let mut response = RunStatusResponse {
        run_state: run.state,
        started_at: Some(run.started_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        updated_at: Some(run.updated_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        summary: Some(run.summary),
        pending_input: None,
        result: None,
    };
    if run.state == RunState::WaitingInput {
        response.pending_input = run.pending_input.clone();
    }
    if is_terminal(run.state) {
        response.result = run.result.clone();
    }

    Ok(Json(response))
}

/// `POST /api/v1/run/stop` (protected, idempotent).
pub async fn stop_run(
    State(state): State<Arc<ServerState>>,
    _body: Option<Json<StopRunRequest>>,
) -> Result<Response, ApiError> {
    state.touch_live_deadline().await;

    let mut active = state.active.lock().await;
    match active.as_mut() {
        None => Ok(run_state_response(StatusCode::OK, RunState::Idle)),
        Some(run) => match run.state {
            RunState::Running | RunState::WaitingInput => {
                run.cancel.cancel();
                run.state = RunState::Stopping;
                run.updated_at = Utc::now();
                Ok(run_state_response(StatusCode::ACCEPTED, RunState::Stopping))
            }
            RunState::Stopping | RunState::Stopped => {
                Ok(run_state_response(StatusCode::ACCEPTED, RunState::Stopping))
            }
            RunState::Finished | RunState::Failed | RunState::Idle => {
                Ok(run_state_response(StatusCode::OK, RunState::Idle))
            }
        },
    }
}

/// `POST /api/v1/run/input` (protected).
pub async fn submit_input(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<SubmitInputRequest>,
) -> Result<Response, ApiError> {
    state.touch_live_deadline().await;

    let mut active = state.active.lock().await;
    let run = active.as_mut().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "INPUT_NOT_FOUND",
            "no run is waiting for input",
        )
    })?;

    let pending = run.pending_input.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "INPUT_NOT_FOUND",
            "no input is pending",
        )
    })?;
    if pending.request_id != request.request_id {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "INPUT_NOT_FOUND",
            "no pending input with the given requestId",
        ));
    }
    if run.state != RunState::WaitingInput {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "INPUT_CONFLICT",
            "input is no longer pending",
        ));
    }

    let answer = validate_answer(pending.kind, request.answer)?;
    let tx = run.input_tx.take().ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "INPUT_CONFLICT",
            "input already answered, cancelled or timed out",
        )
    })?;
    tx.send(super::state::UserResponse::Answer(answer))
        .map_err(|_| {
            ApiError::new(
                StatusCode::CONFLICT,
                "INPUT_CONFLICT",
                "input already answered, cancelled or timed out",
            )
        })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(SubmitInputResponse { accepted: true }),
    )
        .into_response())
}

/// Build the server router.
pub fn router(state: Arc<ServerState>) -> Router {
    let auth_token = Arc::new(state.auth_token.clone());
    let public = Router::new().route("/api/v1/health", get(health));

    let protected = Router::new()
        .route("/api/v1/run", post(start_run))
        .route("/api/v1/run/current", get(run_status))
        .route("/api/v1/run/stop", post(stop_run))
        .route("/api/v1/run/input", post(submit_input))
        .route(
            "/api/v1/config",
            get(super::config::get_config).put(super::config::put_config),
        )
        .route("/api/v1/config/default", post(super::config::config_default))
        .route("/api/v1/fs/list", get(super::fs_api::fs_list))
        .route("/api/v1/fs/read", get(super::fs_api::fs_read))
        .route("/api/v1/fs/write", put(super::fs_api::fs_write))
        .route("/api/v1/fs/mkdir", post(super::fs_api::fs_mkdir))
        .route("/api/v1/fs/remove", delete(super::fs_api::fs_remove))
        .route("/api/v1/fs/move", post(super::fs_api::fs_move))
        .layer(middleware::from_fn_with_state(
            auth_token,
            auth::require_auth,
        ));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            cors_middleware,
        ))
        .with_state(state)
}

async fn cors_middleware(
    State(state): State<Arc<ServerState>>,
    request: Request,
    next: Next,
) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let allowed_origins = {
        let config = state.config.read().await;
        config.server.cors_allowed_origins.clone()
    };
    let allowed_origin = origin
        .as_ref()
        .filter(|origin| allowed_origins.iter().any(|allowed| allowed == *origin));

    if request.method() == Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        if let Some(origin) = allowed_origin {
            add_cors_headers(response.headers_mut(), origin, true);
        }
        return response;
    }

    let mut response = next.run(request).await;
    if let Some(origin) = allowed_origin {
        add_cors_headers(response.headers_mut(), origin, false);
    }
    response
}

fn add_cors_headers(headers: &mut axum::http::HeaderMap, origin: &str, preflight: bool) {
    if let Ok(value) = HeaderValue::from_str(origin) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    if preflight {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Authorization, Content-Type"),
        );
    }
}

fn run_state_response(status: StatusCode, run_state: RunState) -> Response {
    (status, Json(serde_json::json!({ "runState": run_state }))).into_response()
}

fn validate_answer(kind: PendingInputKind, answer: String) -> Result<String, ApiError> {
    match kind {
        PendingInputKind::Text => {
            if answer.trim().is_empty() {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_INPUT",
                    "answer must not be empty",
                ));
            }
            Ok(answer)
        }
        PendingInputKind::Confirm => {
            let normalized = answer.trim().to_lowercase();
            match normalized.as_str() {
                "y" | "yes" => Ok("y".to_string()),
                "n" | "no" => Ok("n".to_string()),
                _ => Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "INVALID_INPUT",
                    "answer must be y, n, yes or no",
                )),
            }
        }
    }
}

