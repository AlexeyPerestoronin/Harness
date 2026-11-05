//! Shared pre-session helpers for console and server runs.
//!
//! Both `harness --prompt ...` and `POST /api/v1/run` must perform the same
//! steps before an agent loop can start: resolve the concrete agent (asking
//! for a 1-based choice when `agent.config` is a directory), ask for an API
//! key when the chosen model has none, and prepare the session `source` with
//! the resolved `agent.config`. This module keeps that logic in one place so
//! the two entry points cannot drift apart.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use conf::{ConfErrorKind, ResolvedAgent, ServerSettings};
use interactions::{Interaction, InteractionError, InteractionKind, InteractionRequest};

/// Failure while resolving the agent or API key before a session starts.
#[derive(Debug)]
pub(crate) enum ResolveAgentError {
    /// The run was cancelled while asking for an agent choice or an API key.
    Cancelled,
    /// The resolution failed for any other reason.
    Failed(String),
}

impl std::fmt::Display for ResolveAgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("interaction cancelled"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// Resolve the agent for a new run, asking the user when necessary.
///
/// When `agent.config` is a directory with several candidates the user gets up
/// to [`agent_loop::agent_select::MAX_ATTEMPTS`] attempts to pick one. When
/// the chosen model has an empty API key the user is asked for a key unless
/// `api_key_override` was supplied by the CLI.
pub(crate) async fn resolve_agent_for_run_interactive(
    settings: &ServerSettings,
    interaction: &Arc<dyn Interaction>,
    api_key_override: Option<String>,
) -> Result<ResolvedAgent, ResolveAgentError> {
    let mut choice: Option<String> = None;
    let mut api_key_override = api_key_override;

    loop {
        match conf::resolve_agent_for_run(settings, choice.as_deref(), api_key_override.as_deref())
        {
            Ok(resolved) => {
                if resolved.agent.api_key.is_empty() && api_key_override.is_none() {
                    let key =
                        prompt_api_key(interaction, &resolved.agent.vendor, &resolved.agent.name)
                            .await?;
                    api_key_override = Some(key);
                    continue;
                }
                return Ok(resolved);
            }
            Err(error) if error.is_agent_choice_required() => {
                let candidates = match error.kind() {
                    ConfErrorKind::AgentChoiceRequired(candidates) => candidates.clone(),
                    _ => return Err(ResolveAgentError::Failed(error.to_string())),
                };
                match select_agent(interaction, &candidates).await? {
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

/// Ask the user to pick a candidate; returns a 0-based index.
///
/// A cancelled interaction is reported as [`ResolveAgentError::Cancelled`] so
/// the server can stop the run while the console reports an error.
async fn select_agent(
    interaction: &Arc<dyn Interaction>,
    candidates: &[conf::AgentCandidate],
) -> Result<Option<usize>, ResolveAgentError> {
    let prompt = agent_loop::agent_select::build_prompt(candidates);
    for _ in 0..agent_loop::agent_select::MAX_ATTEMPTS {
        let request = InteractionRequest {
            kind: InteractionKind::Text,
            prompt: prompt.clone(),
            sensitive: false,
        };
        match interaction.input(&request, true).await {
            Ok(answer) => {
                if let Some(index) =
                    agent_loop::agent_select::parse_choice(answer.trim(), candidates.len())
                {
                    return Ok(Some(index));
                }
            }
            Err(InteractionError::Cancelled) => return Err(ResolveAgentError::Cancelled),
            Err(error) => {
                return Err(ResolveAgentError::Failed(format!(
                    "cannot select agent -> {error}"
                )))
            }
        }
    }
    Ok(None)
}

/// Ask the user for an API key without writing the answer to the log.
pub(crate) async fn prompt_api_key(
    interaction: &Arc<dyn Interaction>,
    vendor: &str,
    name: &str,
) -> Result<String, ResolveAgentError> {
    let request = InteractionRequest {
        kind: InteractionKind::Text,
        prompt: format!("Enter API key for {vendor} {name}: "),
        sensitive: true,
    };
    match interaction.input(&request, false).await {
        Ok(key) if key.trim().is_empty() => Err(ResolveAgentError::Failed(
            "api-key is required".to_string(),
        )),
        Ok(key) => Ok(key),
        Err(InteractionError::Cancelled) => Err(ResolveAgentError::Cancelled),
        Err(error) => Err(ResolveAgentError::Failed(format!(
            "cannot prompt for api key: {error}"
        ))),
    }
}

/// Return `settings.source` with `agent.config` rewritten to the resolved file.
pub(crate) fn source_with_agent_config(
    settings: &ServerSettings,
    config_path: &Path,
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

/// Resolve a CLI `--api-key` value: a literal key or the contents of a file.
///
/// Relative file paths are resolved against `cwd`; the file contents are
/// trimmed before use.
pub(crate) fn resolve_api_key_override(cwd: &Path, raw: &str) -> Result<String, String> {
    let candidate = PathBuf::from(raw);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    };
    if path.is_file() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read api-key file '{}': {e}", path.display()))?;
        Ok(contents.trim().to_string())
    } else {
        Ok(raw.to_string())
    }
}

/// Resolve the effective CWD for a run.
///
/// `--cwd` is resolved against the process current directory; without
/// `--cwd` the process current directory is used. The result is absolute and
/// lexically normalized.
pub(crate) fn resolve_cwd(raw: Option<&Path>) -> Result<PathBuf, String> {
    let current = std::env::current_dir().map_err(|e| e.to_string())?;
    let absolute = match raw {
        Some(raw) if raw.is_absolute() => raw.to_path_buf(),
        Some(raw) => current.join(raw),
        None => current,
    };
    normalize_abs(&absolute).map_err(|message| message.replace("path must be", "'cwd' must be"))
}

/// Resolve a CLI `--conf` / `--dump` path for opening and display.
///
/// Relative paths are resolved against the **process** current directory;
/// absolute paths are kept as-is. The result is absolute and lexically
/// normalized.
pub(crate) fn resolve_abs_path(raw: &Path) -> Result<PathBuf, String> {
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(raw)
    };
    normalize_abs(&absolute)
}

/// Normalize an absolute path: resolve `.` / `..` lexically and require a
/// non-empty result.
pub(crate) fn normalize_abs(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("path must be an absolute path".to_string());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    if normalized.as_os_str().is_empty() {
        Err("path must be set explicitly".to_string())
    } else {
        Ok(normalized)
    }
}
