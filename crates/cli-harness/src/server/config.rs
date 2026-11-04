//! Config REST API (`/api/v1/config`).
//!
//! The handlers read and write `harness.json5` through the `conf` crate, so
//! validation is exactly the same as [`conf::load_server_config`]. Reads are
//! redacted; writes validate before saving and apply the secret merge rule so
//! redaction markers never overwrite real secret values.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use conf::{ServerConfigState, ServerSettings};
use harness_api::{
    ConfigDefaultResponse, ConfigInvalidResponse, ConfigResponse, ConfigState, ErrorBody,
    PutConfigRequest, PutConfigResponse,
};
use serde_json::Value;

use super::handlers::ApiError;
use super::secret;
use super::state::ServerState;

/// `GET /api/v1/config` (protected, does not reset live-timeout).
pub async fn get_config(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<ConfigResponse>, ApiError> {
    let (path, cwd) = state_cwd_and_path(&state).await;
    let path_text = path.to_string_lossy().into_owned();

    match conf::load_server_config(&path, &cwd) {
        Ok(ServerConfigState::Ok(settings)) => {
            let text = serde_json::to_string_pretty(&secret::redact(&settings.source))
                .map_err(|error| internal("cannot serialize config", error))?;
            Ok(Json(ConfigResponse {
                path: path_text,
                exists: true,
                text: Some(text),
                valid: true,
                validation_error: None,
            }))
        }
        Ok(ServerConfigState::Invalid { message }) => Ok(Json(ConfigResponse {
            path: path_text,
            exists: true,
            text: None,
            valid: false,
            validation_error: Some(message),
        })),
        Ok(ServerConfigState::Missing) => Ok(Json(ConfigResponse {
            path: path_text,
            exists: false,
            text: Some(conf::template_text()),
            valid: false,
            validation_error: None,
        })),
        Err(error) => Err(internal("cannot read config", error)),
    }
}

/// `PUT /api/v1/config` (protected, resets live-timeout).
pub async fn put_config(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<PutConfigRequest>,
) -> Result<Response, ApiError> {
    state.touch_live_deadline().await;

    let (path, cwd) = state_cwd_and_path(&state).await;

    let mut parsed = match validate_config_text(&request.text, &cwd, &path) {
        Ok(settings) => settings.source,
        Err(message) => return Ok(config_invalid(message)),
    };

    let text_to_save = if secret::has_redacted_secret(&parsed) {
        let existing = read_existing_parsed(&path, &cwd);
        let Some(existing) = existing else {
            return Ok(config_invalid(
                "redacted secret value cannot be used for a new config".to_string(),
            ));
        };
        if let Err(message) = secret::merge_redacted_secrets(&mut parsed, &existing) {
            return Ok(config_invalid(message));
        }
        let canonical = serde_json::to_string_pretty(&parsed)
            .map_err(|error| internal("cannot serialize config", error))?;
        if let Err(message) = validate_config_text(&canonical, &cwd, &path) {
            return Ok(config_invalid(message));
        }
        canonical
    } else {
        request.text.clone()
    };

    write_config_file(&path, &text_to_save)?;
    reload_server_config(&state, &path).await?;

    Ok((
        StatusCode::OK,
        Json(PutConfigResponse {
            ok: true,
            path: Some(path.to_string_lossy().into_owned()),
            valid: Some(true),
            error: None,
        }),
    )
        .into_response())
}

/// `POST /api/v1/config/default` (protected, resets live-timeout).
///
/// Creates `harness.json5` from the default template when it is missing and
/// returns the current (redacted) configuration when it already exists.
pub async fn config_default(
    State(state): State<Arc<ServerState>>,
) -> Result<Json<ConfigDefaultResponse>, ApiError> {
    state.touch_live_deadline().await;

    let (path, cwd) = state_cwd_and_path(&state).await;
    let path_text = path.to_string_lossy().into_owned();

    if !path.exists() {
        write_config_file(&path, &conf::template_text())?;
        refresh_server_config(&state, &path).await;
        return Ok(Json(ConfigDefaultResponse {
            path: path_text,
            exists: true,
            text: conf::template_text(),
            valid: true,
        }));
    }

    match conf::load_server_config(&path, &cwd) {
        Ok(ServerConfigState::Ok(settings)) => {
            let text = serde_json::to_string_pretty(&secret::redact(&settings.source))
                .map_err(|error| internal("cannot serialize config", error))?;
            Ok(Json(ConfigDefaultResponse {
                path: path_text,
                exists: true,
                text,
                valid: true,
            }))
        }
        Ok(ServerConfigState::Invalid { .. }) => Ok(Json(ConfigDefaultResponse {
            path: path_text,
            exists: true,
            text: String::new(),
            valid: false,
        })),
        Ok(ServerConfigState::Missing) => {
            write_config_file(&path, &conf::template_text())?;
            refresh_server_config(&state, &path).await;
            Ok(Json(ConfigDefaultResponse {
                path: path_text,
                exists: true,
                text: conf::template_text(),
                valid: true,
            }))
        }
        Err(error) => Err(internal("cannot read config", error)),
    }
}

fn read_existing_parsed(path: &Path, cwd: &Path) -> Option<Value> {
    match conf::load_server_config(path, cwd) {
        Ok(ServerConfigState::Ok(settings)) => return Some(settings.source),
        Ok(ServerConfigState::Invalid { .. }) | Ok(ServerConfigState::Missing) => {}
        Err(_) => {}
    }

    let text = std::fs::read_to_string(path).ok()?;
    json5::from_str::<Value>(&text).ok()
}

fn validate_config_text(
    text: &str,
    cwd: &Path,
    target_path: &Path,
) -> Result<ServerSettings, String> {
    let conf_dir = config_path_parent(target_path);
    if !conf_dir.as_os_str().is_empty() {
        std::fs::create_dir_all(&conf_dir)
            .map_err(|error| format!("cannot create config directory: {error}"))?;
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = conf_dir.join(format!(
        ".harness-config-validate-{}-{unique}.json5",
        target_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config")
    ));

    std::fs::write(&temp_path, text)
        .map_err(|error| format!("cannot write validation config: {error}"))?;

    let result = match conf::load_server_config(&temp_path, cwd) {
        Ok(ServerConfigState::Ok(settings)) => Ok(settings),
        Ok(ServerConfigState::Invalid { message }) => Err(message),
        Ok(ServerConfigState::Missing) => Err("config file is missing".to_string()),
        Err(error) => Err(error.to_string()),
    };

    let _ = std::fs::remove_file(&temp_path);
    result
}

fn write_config_file(path: &Path, text: &str) -> Result<(), ApiError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                ApiError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "CONFIG_WRITE_ERROR",
                    format!("cannot create config directory '{}': {error}", parent.display()),
                )
            })?;
        }
    }
    std::fs::write(path, text).map_err(|error| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CONFIG_WRITE_ERROR",
            format!("cannot write config '{}': {error}", path.display()),
        )
    })
}

async fn reload_server_config(state: &Arc<ServerState>, path: &Path) -> Result<(), ApiError> {
    let cwd = state.config.read().await.cwd.clone();
    let loaded = conf::load_server_config(path, &cwd)
        .map_err(|error| internal("cannot reload config", error))?;
    let mut config = state.config.write().await;
    match loaded {
        ServerConfigState::Ok(settings) => {
            config.config_state = ConfigState::Ok;
            config.server = settings.server.clone();
            config.settings = Some(settings);
            Ok(())
        }
        ServerConfigState::Missing => Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CONFIG_WRITE_ERROR",
            "config file disappeared after save",
        )),
        ServerConfigState::Invalid { message } => Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CONFIG_WRITE_ERROR",
            format!("saved config is invalid: {message}"),
        )),
    }
}

async fn refresh_server_config(state: &Arc<ServerState>, path: &Path) {
    let cwd = state.config.read().await.cwd.clone();
    let loaded = match conf::load_server_config(path, &cwd) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!(
                "warning: cannot read configuration '{}': {error}",
                path.display()
            );
            let mut config = state.config.write().await;
            config.config_state = ConfigState::Invalid;
            config.settings = None;
            return;
        }
    };

    let mut config = state.config.write().await;
    match loaded {
        ServerConfigState::Ok(settings) => {
            config.config_state = ConfigState::Ok;
            config.server = settings.server.clone();
            config.settings = Some(settings);
        }
        ServerConfigState::Missing => {
            config.config_state = ConfigState::Missing;
            config.settings = None;
        }
        ServerConfigState::Invalid { message } => {
            eprintln!(
                "warning: invalid configuration '{}': {message}",
                path.display()
            );
            config.config_state = ConfigState::Invalid;
            config.settings = None;
        }
    }
}

async fn state_cwd_and_path(state: &Arc<ServerState>) -> (PathBuf, PathBuf) {
    let config = state.config.read().await;
    (config.path.clone(), config.cwd.clone())
}

fn config_path_parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn config_invalid(message: impl Into<String>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ConfigInvalidResponse {
            ok: false,
            error: ErrorBody {
                code: "CONFIG_INVALID".to_string(),
                message: message.into(),
            },
        }),
    )
        .into_response()
}

fn internal(message: impl Into<String>, error: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "CONFIG_ERROR",
        format!("{}: {error}", message.into()),
    )
}
