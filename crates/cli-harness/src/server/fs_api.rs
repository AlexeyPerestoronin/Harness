//! FS REST API (`/api/v1/fs/*`) scoped to the server `cwd`.
//!
//! Every path is relative to the configured `cwd`, lexically normalized and
//! then canonicalized so symlinks cannot escape the workspace. A deny-list is
//! checked both on the normalized relative path and on the canonicalized
//! result before any filesystem access. JSON5/JSON files are redacted on read
//! and support the secret merge rule on write; other text files are returned
//! and saved as-is.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use harness_api::{
    ConfigState, FsEntry, FsListResponse, FsMkdirResponse, FsMoveResponse, FsReadResponse,
    FsRemoveResponse, FsWriteRequest, FsWriteResponse,
};
use serde_json::Value;

use super::handlers::ApiError;
use super::secret;
use super::state::ServerState;

/// Normalized filesystem path produced from a user-supplied relative path.
struct FsPath {
    /// Lexically normalized relative path (empty means `cwd` itself).
    rel: PathBuf,
    /// Canonical path inside `cwd`; missing suffixes are appended to the
    /// nearest existing canonical ancestor.
    canonical: PathBuf,
    /// Canonical `cwd` used to evaluate deny-list for canonical entry paths.
    cwd_canonical: PathBuf,
    /// Configured session log folder, used to filter protected entries.
    log_folder: String,
}

/// `GET /api/v1/fs/list` (protected, does not reset live-timeout).
pub async fn fs_list(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<FsListResponse>, ApiError> {
    let raw_path = params.get("path").map(String::as_str).unwrap_or("");
    let path = resolve_fs_path(&state, raw_path, true).await?;

    let metadata = std::fs::metadata(&path.canonical)
        .map_err(|_| not_found(format!("path not found: {raw_path}")))?;
    if !metadata.is_dir() {
        return Err(fs_io(format!("path is not a directory: {raw_path}")));
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&path.canonical).map_err(|error| fs_io(error.to_string()))? {
        let entry = entry.map_err(|error| fs_io(error.to_string()))?;
        let file_type = entry
            .file_type()
            .map_err(|error| fs_io(error.to_string()))?;
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let entry_rel = if path.rel.as_os_str().is_empty() {
            PathBuf::from(&name)
        } else {
            path.rel.join(&name)
        };
        let entry_canonical = path.canonical.join(&name);
        let Ok(canonical_rel) = entry_canonical.strip_prefix(&path.cwd_canonical) else {
            continue;
        };
        if is_denied_path(&entry_rel, &path.log_folder)
            || is_denied_path(canonical_rel, &path.log_folder)
        {
            continue;
        }

        let kind = if file_type.is_dir() { "dir" } else { "file" };
        let size = if file_type.is_file() {
            entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
        } else {
            0
        };
        entries.push(FsEntry {
            name,
            kind: kind.to_string(),
            size,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(Json(FsListResponse {
        path: raw_path.to_string(),
        entries,
    }))
}

/// `GET /api/v1/fs/read` (protected, does not reset live-timeout).
pub async fn fs_read(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<FsReadResponse>, ApiError> {
    let raw_path = params.get("path").map(String::as_str).unwrap_or("");
    let path = resolve_fs_path(&state, raw_path, false).await?;

    let metadata = std::fs::metadata(&path.canonical)
        .map_err(|_| not_found(format!("path not found: {raw_path}")))?;
    if metadata.is_dir() {
        return Err(fs_io(format!("path is a directory: {raw_path}")));
    }

    let text = std::fs::read_to_string(&path.canonical)
        .map_err(|_| fs_io(format!("file is not valid UTF-8: {raw_path}")))?;
    let text = match json5::from_str::<Value>(&text) {
        Ok(value) => serde_json::to_string_pretty(&secret::redact(&value))
            .map_err(|error| fs_io(format!("cannot serialize JSON: {error}")))?,
        Err(_) => text,
    };

    Ok(Json(FsReadResponse {
        path: raw_path.to_string(),
        text,
    }))
}

/// `PUT /api/v1/fs/write` (protected, resets live-timeout).
pub async fn fs_write(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<FsWriteRequest>,
) -> Result<Json<FsWriteResponse>, ApiError> {
    state.touch_live_deadline().await;

    let raw_path = params.get("path").map(String::as_str).unwrap_or("");
    let path = resolve_fs_path(&state, raw_path, false).await?;

    if path.canonical.exists() && path.canonical.is_dir() {
        return Err(fs_io(format!("path is a directory: {raw_path}")));
    }

    let existing_parsed = if path.canonical.exists() {
        std::fs::read_to_string(&path.canonical)
            .ok()
            .and_then(|text| json5::from_str::<Value>(&text).ok())
    } else {
        None
    };

    let text_to_save = match json5::from_str::<Value>(&request.text) {
        Ok(mut parsed) if secret::has_redacted_secret(&parsed) => {
            let Some(existing) = existing_parsed else {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "REDACTED_SECRET",
                    "redacted secret value cannot be used for a new file",
                ));
            };
            secret::merge_redacted_secrets(&mut parsed, &existing).map_err(|message| {
                ApiError::new(StatusCode::BAD_REQUEST, "REDACTED_SECRET", message)
            })?;
            serde_json::to_string_pretty(&parsed)
                .map_err(|error| fs_io(format!("cannot serialize JSON: {error}")))?
        }
        _ => request.text.clone(),
    };

    write_text_file(&path.canonical, &text_to_save)?;

    Ok(Json(FsWriteResponse {
        ok: true,
        path: raw_path.to_string(),
    }))
}

/// `POST /api/v1/fs/mkdir` (protected, resets live-timeout).
pub async fn fs_mkdir(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<FsMkdirResponse>, ApiError> {
    state.touch_live_deadline().await;

    let raw_path = params.get("path").map(String::as_str).unwrap_or("");
    let path = resolve_fs_path(&state, raw_path, false).await?;

    if path.canonical.exists() {
        if path.canonical.is_dir() {
            return Ok(Json(FsMkdirResponse {
                ok: true,
                path: raw_path.to_string(),
            }));
        }
        return Err(fs_io(format!("path already exists and is not a directory: {raw_path}")));
    }

    std::fs::create_dir_all(&path.canonical)
        .map_err(|error| fs_io(format!("cannot create directory: {error}")))?;

    Ok(Json(FsMkdirResponse {
        ok: true,
        path: raw_path.to_string(),
    }))
}

/// `DELETE /api/v1/fs/remove` (protected, resets live-timeout).
pub async fn fs_remove(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<FsRemoveResponse>, ApiError> {
    state.touch_live_deadline().await;

    let raw_path = params.get("path").map(String::as_str).unwrap_or("");
    let path = resolve_fs_path(&state, raw_path, false).await?;

    let metadata = std::fs::metadata(&path.canonical)
        .map_err(|_| not_found(format!("path not found: {raw_path}")))?;

    if metadata.is_dir() {
        std::fs::remove_dir(&path.canonical).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                not_found(format!("path not found: {raw_path}"))
            } else {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "DIR_NOT_EMPTY",
                    format!("cannot remove directory '{}': {error}", raw_path),
                )
            }
        })?;
    } else if metadata.is_file() {
        std::fs::remove_file(&path.canonical)
            .map_err(|error| fs_io(format!("cannot remove file '{}': {error}", raw_path)))?;
    } else {
        return Err(fs_io(format!("unsupported file type: {raw_path}")));
    }

    Ok(Json(FsRemoveResponse {
        ok: true,
        path: raw_path.to_string(),
    }))
}

/// `POST /api/v1/fs/move` (protected, resets live-timeout).
pub async fn fs_move(
    State(state): State<Arc<ServerState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<FsMoveResponse>, ApiError> {
    state.touch_live_deadline().await;

    let raw_src = params.get("src").map(String::as_str).unwrap_or("");
    let raw_dst = params.get("dst").map(String::as_str).unwrap_or("");
    let src = resolve_fs_path(&state, raw_src, false).await?;
    let dst = resolve_fs_path(&state, raw_dst, false).await?;

    if !src.canonical.exists() {
        return Err(not_found(format!("source path not found: {raw_src}")));
    }

    if let Some(parent) = dst.canonical.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|error| fs_io(format!("cannot create destination directory: {error}")))?;
        }
    }

    std::fs::rename(&src.canonical, &dst.canonical)
        .map_err(|error| fs_io(format!("cannot move '{}' to '{}': {error}", raw_src, raw_dst)))?;

    Ok(Json(FsMoveResponse {
        ok: true,
        src: raw_src.to_string(),
        dst: raw_dst.to_string(),
    }))
}

async fn resolve_fs_path(
    state: &Arc<ServerState>,
    raw: &str,
    allow_empty: bool,
) -> Result<FsPath, ApiError> {
    let (cwd, log_folder) = require_settings(state).await?;
    let rel = normalize_rel_path(raw).map_err(invalid_path)?;
    if rel.as_os_str().is_empty() && !allow_empty {
        return Err(invalid_path("path must not be empty"));
    }
    if is_denied_path(&rel, &log_folder) {
        return Err(forbidden_path(format!("path is protected: {raw}")));
    }

    let abs = cwd.join(&rel);
    let canonical = canonicalize_inside(&cwd, &abs).map_err(invalid_path)?;

    let cwd_canonical = cwd
        .canonicalize()
        .map_err(|error| invalid_path(format!("cannot access cwd '{}': {error}", cwd.display())))?;
    let canonical_rel = canonical
        .strip_prefix(&cwd_canonical)
        .map_err(|_| invalid_path("path escapes cwd"))?;
    if is_denied_path(canonical_rel, &log_folder) {
        return Err(forbidden_path(format!("path is protected: {raw}")));
    }

    Ok(FsPath {
        rel,
        canonical,
        cwd_canonical,
        log_folder,
    })
}

async fn require_settings(state: &Arc<ServerState>) -> Result<(PathBuf, String), ApiError> {
    let config = state.config.read().await;
    let Some(settings) = config.settings.as_ref() else {
        return Err(match config.config_state {
            ConfigState::Missing => ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "CONFIG_MISSING",
                "harness.json5 does not exist",
            ),
            ConfigState::Invalid => ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "CONFIG_INVALID",
                "harness.json5 is invalid",
            ),
            ConfigState::Ok => ApiError::new(
                StatusCode::PRECONDITION_FAILED,
                "CONFIG_UNAVAILABLE",
                "configuration is not available",
            ),
        });
    };
    Ok((
        settings.cwd.clone(),
        settings.agent_session.log_folder.clone(),
    ))
}

fn normalize_rel_path(raw: &str) -> Result<PathBuf, String> {
    if raw.is_empty() {
        return Ok(PathBuf::new());
    }

    let path = Path::new(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir => return Err("'..' is not allowed".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("absolute paths are not allowed".to_string())
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err("path must not be empty".to_string());
    }
    Ok(normalized)
}

fn is_denied_path(rel: &Path, log_folder: &str) -> bool {
    if rel == Path::new("harness.json5") {
        return true;
    }

    if let Some(Component::Normal(first)) = rel.components().next() {
        if first == ".harness" {
            return true;
        }
    }

    let log_folder = Path::new(log_folder);
    if !log_folder.as_os_str().is_empty() && (rel == log_folder || rel.starts_with(log_folder)) {
        return true;
    }

    if let Some(name) = rel.file_name().and_then(|name| name.to_str()) {
        if matches!(name, ".env" | "id_rsa" | "id_ed25519") {
            return true;
        }
    }

    if let Some(extension) = rel.extension().and_then(|extension| extension.to_str()) {
        if matches!(extension, "pem" | "key" | "p12" | "pfx") {
            return true;
        }
    }

    false
}

fn canonicalize_inside(cwd: &Path, abs: &Path) -> Result<PathBuf, String> {
    let cwd_canonical = cwd
        .canonicalize()
        .map_err(|error| format!("cannot access cwd '{}': {error}", cwd.display()))?;

    let mut probe = abs.to_path_buf();
    let mut suffix = Vec::new();

    loop {
        match probe.canonicalize() {
            Ok(canonical) => {
                let mut resolved = canonical;
                for name in suffix.iter().rev() {
                    resolved.push(name);
                }
                if !resolved.starts_with(&cwd_canonical) {
                    return Err("path escapes cwd".to_string());
                }
                return Ok(resolved);
            }
            Err(_) => {
                let name = probe
                    .file_name()
                    .ok_or_else(|| "invalid path".to_string())?
                    .to_os_string();
                suffix.push(name);
                probe = probe
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .map(Path::to_path_buf)
                    .ok_or_else(|| "invalid path".to_string())?;
            }
        }
    }
}

fn write_text_file(path: &Path, text: &str) -> Result<(), ApiError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|error| fs_io(format!("cannot create parent directory: {error}")))?;
        }
    }
    std::fs::write(path, text)
        .map_err(|error| fs_io(format!("cannot write file '{}': {error}", path.display())))
}

fn invalid_path(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "INVALID_PATH", message)
}

fn forbidden_path(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::FORBIDDEN, "FORBIDDEN_PATH", message)
}

fn not_found(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "FS_NOT_FOUND", message)
}

fn fs_io(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, "FS_IO", message)
}

