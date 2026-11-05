//! Load `harness.json5` and the nested model file into [`crate::Settings`],
//! or partially into [`crate::ServerSettings`] for the HTTP server.
//!
//! A model file must contain `limits` and `price`. Flat legacy keys
//! (`token-limit`, `iteration-limit`, `rpm`, ...) are a load error, not a
//! silent migration. Session limits still come from `harness.json5`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ConfError;
use crate::scan::{scan_agent_dir, SkippedFile};
use crate::str::keys;
use crate::types::{
    AgentCandidate, AgentKind, AgentSession, AgentSettings, AgentSource, FsSettings, GitSettings,
    Mode, ResolvedAgent, RoleEntry, RolesSettings, ServerConfig, ServerConfigState, ServerSettings,
    Settings, SkillEntry, SkillsSettings, TerminalSettings, ToolsSettings, UserSettings,
};
use serde_json::Value;

/// Load and validate a harness configuration file.
///
/// If `agent.config` is a directory this fails with
/// `agent was not selected`. Use [`load_with`] to choose a candidate.
pub fn load(conf_path: &Path, cwd: &Path) -> Result<Settings, ConfError> {
    load_with(conf_path, cwd, |_| Err("agent was not selected".into()))
}

/// Load and validate a harness configuration file.
///
/// When `agent.config` is a **file**, `chooser` is not called.
/// When it is a **directory**, `chooser` receives the sorted candidate list
/// and must return a 0-based index. Session limits (`mode`, `token-limit`,
/// `iteration-limit`, `USD-limit`) always come from `harness.json5`. `cwd`
/// comes from the caller and is the root for every relative config path.
pub fn load_with<F>(conf_path: &Path, cwd: &Path, chooser: F) -> Result<Settings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let conf_path = crate::fs_scope::normalize_abs(conf_path);
    ensure_conf_in_cwd(&conf_path, cwd)?;
    let root = read_root(&conf_path)?;
    let mut settings = load_from_value(root, cwd, chooser)?;
    settings.conf_dir = conf_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| settings.cwd.clone());
    Ok(settings)
}

/// Read, substitute vars, parse and expect a root object from a conf file.
///
/// This is the single read/parse chain shared by [`load_with`] and
/// [`load_server_config`] so that the order of checks and error messages
/// cannot drift between the two loaders.
fn read_root(conf_path: &Path) -> Result<Value, ConfError> {
    let text = fs::read_to_string(conf_path).map_err(|e| {
        ConfError::new(format!(
            "cannot read conf file '{}': {e}",
            conf_path.display()
        ))
    })?;
    let text = crate::vars::substitute(&text, &conf_path.display().to_string())?;
    let root: Value = json5::from_str(&text)?;
    expect_object(&root, &conf_path.display().to_string())?;
    Ok(root)
}

/// Validate a conf JSON object (already after vars substitution) into [`Settings`].
///
/// `vars` is not substituted again. `cwd` comes from the caller and is the
/// root for every relative path. [`Settings::conf_dir`] is set to `cwd`;
/// file-based loaders may overwrite it with the config file directory.
/// After a successful load, [`Settings::source`] is `root` with `agent.config`
/// rewritten to the absolute chosen agent file.
pub fn load_from_value<F>(
    mut root: Value,
    cwd: &Path,
    mut chooser: F,
) -> Result<Settings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let cwd = crate::fs_scope::normalize_abs(cwd);
    let (log_folder, agent, tools) = {
        let obj = expect_object(&root, "settings")?;
        validate_root_keys(obj)?;

        let agent_raw = obj
            .get(keys::AGENT)
            .ok_or_else(|| ConfError::new(format!("missing '{}'", keys::AGENT)))?;
        let agent_obj = expect_object(agent_raw, keys::AGENT)?;
        let log_folder = log_folder_from_agent(agent_obj, &cwd)?;
        let agent = load_agent(agent_obj, &cwd, &mut chooser)?;

        let tools = match obj.get(keys::TOOLS) {
            Some(v) => load_tools(expect_object(v, keys::TOOLS)?, &cwd)?,
            None => ToolsSettings::default(),
        };
        (log_folder, agent, tools)
    };

    rewrite_agent_config(&mut root, &agent.config_path);

    Ok(Settings {
        cwd: cwd.clone(),
        conf_dir: cwd,
        log_folder,
        agent,
        tools,
        source: root,
    })
}

/// Load server-side configuration without selecting an agent or requiring an API key.
///
/// Returns [`ServerConfigState::Missing`] when the file does not exist and
/// [`ServerConfigState::Invalid`] for read / parse / validation errors.
/// Unexpected IO errors from the existence check (for example permission
/// denied on a parent directory) are returned as `Err`.
pub fn load_server_config(conf_path: &Path, cwd: &Path) -> Result<ServerConfigState, ConfError> {
    let conf_path = crate::fs_scope::normalize_abs(conf_path);
    ensure_conf_in_cwd(&conf_path, cwd)?;
    match fs::metadata(&conf_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ServerConfigState::Missing)
        }
        Err(e) => {
            return Err(ConfError::new(format!(
                "cannot read conf file '{}': {e}",
                conf_path.display()
            )))
        }
    }
    let root = match read_root(&conf_path) {
        Ok(root) => root,
        Err(e) => {
            return Ok(ServerConfigState::Invalid {
                message: e.to_string(),
            })
        }
    };

    match server_settings_from_value(root, cwd) {
        Ok(mut settings) => {
            settings.conf_dir = conf_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| settings.cwd.clone());
            Ok(ServerConfigState::Ok(settings))
        }
        Err(e) => Ok(ServerConfigState::Invalid {
            message: e.to_string(),
        }),
    }
}

/// Resolve and load the concrete agent for a `POST /run`.
///
/// For `agent.config = directory` with exactly one candidate the choice is
/// automatic. With several candidates the caller must provide a 1-based
/// `agent_choice`; otherwise [`ConfError::agent_choice_required`] is returned.
/// `api_key_override` allows loading a model with a missing / empty `api-key`.
pub fn resolve_agent_for_run(
    settings: &ServerSettings,
    agent_choice: Option<&str>,
    api_key_override: Option<&str>,
) -> Result<ResolvedAgent, ConfError> {
    let model_path = match &settings.agent_source {
        AgentSource::File { path } => path.clone(),
        AgentSource::Directory { path, candidates } => {
            choose_from_candidates(path, candidates, agent_choice)?
        }
    };
    let model_obj = read_model_object(&model_path)?;
    let agent = settings_from_model_for_run(
        &model_obj,
        settings.agent_session.mode,
        Some((
            settings.agent_session.token_limit,
            settings.agent_session.iteration_limit,
            settings.agent_session.usd_limit,
        )),
        model_path.clone(),
        api_key_override,
        &settings.cwd,
    )?;
    Ok(ResolvedAgent {
        agent,
        config_path: model_path,
    })
}

/// Resolve a config path against `cwd` and require the result to stay inside it.
///
/// Relative paths are joined onto `cwd`; absolute paths are kept as-is. The
/// result is lexically normalized and checked by path components.
pub(crate) fn resolve_in_cwd(raw: &str, cwd: &Path) -> Result<PathBuf, ConfError> {
    let cwd = crate::fs_scope::normalize_abs(cwd);
    let raw = raw.replace('\\', "/");
    let raw_path = Path::new(&raw);
    let joined = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        cwd.join(raw_path)
    };
    let absolute = crate::fs_scope::normalize_lexical(&joined);
    if !is_within_cwd(&absolute, &cwd) {
        return Err(ConfError::new(format!(
            "path '{}' resolves outside of cwd '{}'",
            raw,
            cwd.display()
        )));
    }
    Ok(absolute)
}

fn ensure_conf_in_cwd(conf_path: &Path, cwd: &Path) -> Result<(), ConfError> {
    if !is_within_cwd(conf_path, &crate::fs_scope::normalize_abs(cwd)) {
        return Err(ConfError::new(format!(
            "conf file '{}' is outside of cwd '{}'",
            conf_path.display(),
            cwd.display()
        )));
    }
    Ok(())
}

fn is_within_cwd(path: &Path, cwd: &Path) -> bool {
    let path = crate::fs_scope::comparable_parts(path);
    let cwd = crate::fs_scope::comparable_parts(cwd);
    path.len() >= cwd.len() && path.starts_with(&cwd)
}

fn validate_root_keys(obj: &serde_json::Map<String, Value>) -> Result<(), ConfError> {
    for key in obj.keys() {
        if key == keys::CWD {
            return Err(ConfError::new(format!(
                "'{}' is no longer set in the config; use '--cwd' or run the process from the target directory",
                keys::CWD
            )));
        }
        if !keys::ROOT_KEYS.contains(&key.as_str()) {
            return Err(ConfError::new(format!("unknown root key '{key}'")));
        }
    }
    Ok(())
}

fn strip_rw_prefix(entry: &str) -> &str {
    entry
        .strip_prefix("r:")
        .or_else(|| entry.strip_prefix("w:"))
        .unwrap_or(entry)
}

fn server_settings_from_value(root: Value, cwd: &Path) -> Result<ServerSettings, ConfError> {
    let cwd = crate::fs_scope::normalize_abs(cwd);
    let obj = expect_object(&root, "settings")?;
    validate_root_keys(obj)?;
    let server = match obj.get(keys::SERVER) {
        Some(v) => load_server(expect_object(v, keys::SERVER)?, &cwd)?,
        None => ServerConfig::default(),
    };
    let tools = match obj.get(keys::TOOLS) {
        Some(v) => load_tools(expect_object(v, keys::TOOLS)?, &cwd)?,
        None => ToolsSettings::default(),
    };

    let agent_raw = obj
        .get(keys::AGENT)
        .ok_or_else(|| ConfError::new(format!("missing '{}'", keys::AGENT)))?;
    let agent_obj = expect_object(agent_raw, keys::AGENT)?;
    let mode = parse_mode(agent_obj)?;
    let token_limit = required_i64(agent_obj, keys::TOKEN_LIMIT)?;
    let iteration_limit = required_i64(agent_obj, keys::ITERATION_LIMIT)?;
    let usd_limit = required_f64(agent_obj, keys::USD_LIMIT)?;
    let log_folder = log_folder_from_agent(agent_obj, &cwd)?;

    let config_rel = required_string(agent_obj, keys::CONFIG)?;
    let source_path = resolve_in_cwd(&config_rel, &cwd)?;
    let agent_source = if source_path.is_dir() {
        let (candidates, skipped) = scan_agent_dir(&source_path)?;
        report_skipped(&skipped);
        AgentSource::Directory {
            path: source_path,
            candidates,
        }
    } else {
        if !source_path.exists() {
            return Err(ConfError::new(format!(
                "agent config file '{}' does not exist",
                source_path.display()
            )));
        }
        AgentSource::File { path: source_path }
    };

    Ok(ServerSettings {
        cwd: cwd.clone(),
        conf_dir: cwd,
        server,
        agent_source,
        agent_session: AgentSession {
            mode,
            token_limit,
            iteration_limit,
            usd_limit,
            log_folder,
        },
        tools,
        source: root,
    })
}

fn load_server(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<ServerConfig, ConfError> {
    reject_unknown_keys(
        obj,
        &[
            keys::HOST,
            keys::PORT,
            keys::LIVE_TIMEOUT,
            keys::INTERACTION_TIMEOUT,
            keys::CORS_ALLOWED_ORIGINS,
            keys::TLS_CERT,
            keys::TLS_KEY,
        ],
        keys::SERVER,
    )?;

    let mut server = ServerConfig::default();
    if let Some(v) = obj.get(keys::HOST) {
        server.host = expect_string(v, keys::HOST)?;
    }
    if let Some(v) = obj.get(keys::PORT) {
        server.port = parse_port(v)?;
    }
    if let Some(v) = obj.get(keys::LIVE_TIMEOUT) {
        server.live_timeout_minutes = parse_positive_minutes(v, keys::LIVE_TIMEOUT)?;
    }
    if let Some(v) = obj.get(keys::INTERACTION_TIMEOUT) {
        server.interaction_timeout_minutes = parse_positive_minutes(v, keys::INTERACTION_TIMEOUT)?;
    }
    if let Some(v) = obj.get(keys::CORS_ALLOWED_ORIGINS) {
        server.cors_allowed_origins = parse_cors_origins(v)?;
    }
    server.tls_cert = parse_tls_path(obj.get(keys::TLS_CERT), keys::TLS_CERT, cwd)?;
    server.tls_key = parse_tls_path(obj.get(keys::TLS_KEY), keys::TLS_KEY, cwd)?;
    if server.tls_cert.is_some() != server.tls_key.is_some() {
        return Err(ConfError::new(format!(
            "'{}' and '{}' must be set together or both be null",
            keys::TLS_CERT,
            keys::TLS_KEY
        )));
    }
    Ok(server)
}

fn parse_port(value: &Value) -> Result<u16, ConfError> {
    let n = strict_u64(value)
        .ok_or_else(|| ConfError::new(format!("'{}' must be an integer", keys::PORT)))?;
    let port = u16::try_from(n)
        .ok()
        .filter(|p| *p >= 1)
        .ok_or_else(|| ConfError::new(format!("'{}' must be in range 1..=65535", keys::PORT)))?;
    Ok(port)
}

fn parse_positive_minutes(value: &Value, key: &str) -> Result<u64, ConfError> {
    let n = strict_u64(value)
        .ok_or_else(|| ConfError::new(format!("'{key}' must be a positive integer")))?;
    if n == 0 {
        return Err(ConfError::new(format!(
            "'{key}' must be a positive integer"
        )));
    }
    Ok(n)
}

fn parse_cors_origins(value: &Value) -> Result<Vec<String>, ConfError> {
    let items = match value {
        Value::Array(items) => items,
        _ => {
            return Err(ConfError::new(format!(
                "'{}' must be an array of strings",
                keys::CORS_ALLOWED_ORIGINS
            )))
        }
    };
    let mut origins = Vec::with_capacity(items.len());
    for item in items {
        let origin = expect_string(item, keys::CORS_ALLOWED_ORIGINS)?;
        if origin == "*" {
            return Err(ConfError::new(format!(
                "'{}' must not contain \"*\"",
                keys::CORS_ALLOWED_ORIGINS
            )));
        }
        origins.push(origin);
    }
    Ok(origins)
}

fn parse_tls_path(
    value: Option<&Value>,
    key: &str,
    cwd: &Path,
) -> Result<Option<PathBuf>, ConfError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let path = PathBuf::from(s);
            let path = if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            };
            Ok(Some(crate::fs_scope::normalize_lexical(&path)))
        }
        Some(_) => Err(ConfError::new(format!("'{key}' must be a string or null"))),
    }
}

fn choose_from_candidates(
    dir: &Path,
    candidates: &[AgentCandidate],
    agent_choice: Option<&str>,
) -> Result<PathBuf, ConfError> {
    if candidates.is_empty() {
        let (_, skipped) = scan_agent_dir(dir)?;
        return Err(no_agent_candidates_error(dir, &skipped));
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].path.clone());
    }
    let Some(choice) = agent_choice else {
        return Err(ConfError::agent_choice_required(candidates.to_vec()));
    };
    match parse_agent_choice(choice, candidates.len()) {
        Some(index) => Ok(candidates[index].path.clone()),
        None => Err(ConfError::invalid_agent_choice(format!(
            "invalid agent choice '{choice}': expected a number from 1 to {}",
            candidates.len()
        ))),
    }
}

fn parse_agent_choice(choice: &str, candidates_len: usize) -> Option<usize> {
    let parsed = choice.trim().parse::<usize>().ok()?;
    if parsed >= 1 && parsed <= candidates_len {
        Some(parsed - 1)
    } else {
        None
    }
}

fn no_agent_candidates_error(dir: &Path, skipped: &[SkippedFile]) -> ConfError {
    let mut msg = format!("no agent config files found in '{}'", dir.display());
    for s in skipped {
        msg.push_str(&format!("\nskipped '{}': {}", s.file, s.reason));
    }
    ConfError::new(msg)
}

fn rewrite_agent_config(root: &mut Value, config_path: &Path) {
    if let Some(agent) = root
        .as_object_mut()
        .and_then(|o| o.get_mut(keys::AGENT))
        .and_then(|v| v.as_object_mut())
    {
        agent.insert(
            keys::CONFIG.to_string(),
            Value::String(config_path.to_string_lossy().into_owned()),
        );
    }
}

fn read_model_object(model_path: &Path) -> Result<serde_json::Map<String, Value>, ConfError> {
    let model_text = fs::read_to_string(model_path).map_err(|e| {
        ConfError::new(format!(
            "cannot read model file '{}': {e}",
            model_path.display()
        ))
    })?;
    let model_text = crate::vars::substitute(&model_text, &model_path.display().to_string())?;
    let model_root: Value = json5::from_str(&model_text)?;
    expect_object(&model_root, &model_path.display().to_string()).cloned()
}

fn load_agent<F>(
    agent_obj: &serde_json::Map<String, Value>,
    cwd: &Path,
    chooser: &mut F,
) -> Result<AgentSettings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let config_rel = required_string(agent_obj, keys::CONFIG)?;
    let mode = parse_mode(agent_obj)?;

    let token_limit = required_i64(agent_obj, keys::TOKEN_LIMIT)?;
    let iteration_limit = required_i64(agent_obj, keys::ITERATION_LIMIT)?;
    let usd_limit = required_f64(agent_obj, keys::USD_LIMIT)?;

    let model_path = resolve_in_cwd(&config_rel, cwd)?;
    let model_path = if model_path.is_dir() {
        choose_from_dir(&model_path, chooser)?
    } else {
        model_path
    };
    let model_obj = match read_model_object(&model_path) {
        Ok(obj) => obj,
        Err(e) => {
            if !model_path.exists() && model_path.extension().is_none() {
                return Err(ConfError::new(format!(
                    "{e} (expected a file or a directory)"
                )));
            }
            return Err(e);
        }
    };
    settings_from_model(
        &model_obj,
        mode,
        Some((token_limit, iteration_limit, usd_limit)),
        model_path,
        cwd,
    )
}

fn parse_mode(agent_obj: &serde_json::Map<String, Value>) -> Result<Mode, ConfError> {
    let mode_raw = required_string(agent_obj, keys::MODE)?;
    match mode_raw.as_str() {
        v if v == keys::MODE_SAFE => Ok(Mode::Safe),
        v if v == keys::MODE_AUTO_ACCEPT => Ok(Mode::AutoAccept),
        other => Err(ConfError::new(format!(
            "invalid '{}': '{other}' (expected '{}' or '{}')",
            keys::MODE,
            keys::MODE_SAFE,
            keys::MODE_AUTO_ACCEPT
        ))),
    }
}

fn choose_from_dir<F>(dir: &Path, chooser: &mut F) -> Result<PathBuf, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let (candidates, skipped) = scan_agent_dir(dir)?;
    report_skipped(&skipped);
    if candidates.is_empty() {
        return Err(no_agent_candidates_error(dir, &skipped));
    }
    let index = chooser(&candidates).map_err(ConfError::new)?;
    let chosen = candidates
        .get(index)
        .ok_or_else(|| ConfError::new("agent was not selected".to_string()))?;
    Ok(chosen.path.clone())
}

fn report_skipped(skipped: &[SkippedFile]) {
    for s in skipped {
        eprintln!("skipped '{}': {}", s.file, s.reason);
    }
}

fn settings_from_model(
    model_obj: &serde_json::Map<String, Value>,
    mode: Mode,
    session_limits: Option<(i64, i64, f64)>,
    config_path: PathBuf,
    cwd: &Path,
) -> Result<AgentSettings, ConfError> {
    let api_key = resolve_api_key(model_obj, cwd)?;
    build_agent_settings(model_obj, mode, session_limits, config_path, api_key)
}

fn settings_from_model_for_run(
    model_obj: &serde_json::Map<String, Value>,
    mode: Mode,
    session_limits: Option<(i64, i64, f64)>,
    config_path: PathBuf,
    api_key_override: Option<&str>,
    cwd: &Path,
) -> Result<AgentSettings, ConfError> {
    let api_key = resolve_api_key_for_run(model_obj, api_key_override, cwd)?;
    build_agent_settings(model_obj, mode, session_limits, config_path, api_key)
}

fn build_agent_settings(
    model_obj: &serde_json::Map<String, Value>,
    mode: Mode,
    session_limits: Option<(i64, i64, f64)>,
    config_path: PathBuf,
    api_key: String,
) -> Result<AgentSettings, ConfError> {
    let numbers = read_model_numbers(model_obj, true)?;
    let vendor = required_string(model_obj, keys::VENDOR)?;
    let name = required_string(model_obj, keys::NAME)?;
    let kind = AgentKind::from_vendor_name(&vendor, &name)?;

    let (token_limit, iteration_limit, usd_limit) =
        session_limits.unwrap_or((numbers.token_limit, -1, numbers.usd_limit));

    Ok(AgentSettings {
        vendor,
        name,
        api_key,
        kind,
        mode,
        token_limit,
        iteration_limit,
        usd_limit,
        rpm: numbers.rpm,
        tpm: numbers.tpm,
        rpd: numbers.rpd,
        input_price: numbers.input_price,
        output_price: numbers.output_price,
        cached_price: numbers.cached_price,
        config_path,
    })
}

fn resolve_api_key(
    model_obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<String, ConfError> {
    let raw = match model_obj.get(keys::API_KEY) {
        None => {
            return Err(ConfError::new(format!(
                "please set '{}' in the model file",
                keys::API_KEY
            )))
        }
        Some(v) => expect_string(v, keys::API_KEY)?,
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    resolve_api_key_value(trimmed, cwd)
}

fn resolve_api_key_for_run(
    model_obj: &serde_json::Map<String, Value>,
    api_key_override: Option<&str>,
    cwd: &Path,
) -> Result<String, ConfError> {
    if let Some(raw) = api_key_override {
        return Ok(raw.trim().to_string());
    }
    match model_obj.get(keys::API_KEY) {
        None => Ok(String::new()),
        Some(v) => {
            let raw = expect_string(v, keys::API_KEY)?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(String::new())
            } else {
                resolve_api_key_value(trimmed, cwd)
            }
        }
    }
}

fn resolve_api_key_value(trimmed: &str, cwd: &Path) -> Result<String, ConfError> {
    let candidate = PathBuf::from(trimmed);
    let cwd = crate::fs_scope::normalize_abs(cwd);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    };
    let path = crate::fs_scope::normalize_lexical(&path);
    if path.is_file() {
        let contents = fs::read_to_string(&path).map_err(|e| {
            ConfError::new(format!(
                "cannot read '{}' file '{}': {e}",
                keys::API_KEY,
                path.display()
            ))
        })?;
        return Ok(contents.trim().to_string());
    }
    Ok(trimmed.to_string())
}
fn load_tools(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<ToolsSettings, ConfError> {
    let mut settings = ToolsSettings::default();
    for (name, value) in obj {
        let module = expect_object(value, name)?;
        if name == keys::TOOL_FS {
            settings.fs = Some(load_fs(module, cwd)?);
        } else if name == keys::TOOL_GIT {
            settings.git = Some(load_git(module)?);
        } else if name == keys::TOOL_TERMINAL {
            settings.terminal = Some(load_terminal(module, cwd)?);
        } else if name == keys::TOOL_USER {
            settings.user = Some(UserSettings {});
        } else if name == keys::TOOL_SKILLS {
            settings.skills = Some(load_skills(module, cwd)?);
        } else if name == keys::TOOL_ROLES {
            settings.roles = Some(load_roles(module, cwd)?);
        } else {
            return Err(ConfError::new(format!("unknown tool module '{name}'")));
        }
    }
    Ok(settings)
}

fn load_fs(obj: &serde_json::Map<String, Value>, cwd: &Path) -> Result<FsSettings, ConfError> {
    if obj.contains_key(keys::EXTRA_SCOPE) {
        return Err(ConfError::new(format!(
            "'{}' was renamed to '{}'",
            keys::EXTRA_SCOPE,
            keys::AVAILABLE_FILES
        )));
    }
    let available_dirs = optional_string_list(obj, keys::AVAILABLE_DIRS)?;
    let available_file_extensions = optional_string_list(obj, keys::AVAILABLE_FILE_EXTENSIONS)?;
    let available_files = optional_string_list(obj, keys::AVAILABLE_FILES)?;
    let deprecated_dirs = required_deprecated_list(obj, keys::DEPRECATED_DIRS)?;
    let deprecated_files = required_deprecated_list(obj, keys::DEPRECATED_FILES)?;
    let deprecated_dir_patterns = optional_string_list(obj, keys::DEPRECATED_DIR_PATTERNS)?;

    // `tools.fs` paths are validated twice on purpose: here in `load_fs`
    // for early per-entry errors and again in `fs_scope::check_conflicts`.
    // Both must use `resolve_in_cwd` and keep the same canonical error text
    // `resolves outside of cwd`, otherwise the messages can drift.
    for item in &available_dirs {
        resolve_in_cwd(strip_rw_prefix(item), cwd)?;
    }
    for item in &available_files {
        resolve_in_cwd(strip_rw_prefix(item), cwd)?;
    }
    for item in &deprecated_dirs {
        resolve_in_cwd(item, cwd)?;
    }
    for item in &deprecated_files {
        resolve_in_cwd(item, cwd)?;
    }
    for pattern in &deprecated_dir_patterns {
        let source = crate::glob::regex_source(pattern)?;
        regex::Regex::new(&source).map_err(|e| {
            ConfError::new(format!(
                "invalid '{}' entry '{pattern}': cannot compile glob pattern: {e}",
                keys::DEPRECATED_DIR_PATTERNS
            ))
        })?;
    }

    crate::fs_scope::check_conflicts(
        cwd,
        &available_dirs,
        &available_files,
        &deprecated_dirs,
        &deprecated_files,
    )?;
    Ok(FsSettings {
        available_dirs,
        available_file_extensions,
        available_files,
        deprecated_dirs,
        deprecated_files,
        deprecated_dir_patterns,
    })
}

fn load_git(obj: &serde_json::Map<String, Value>) -> Result<GitSettings, ConfError> {
    let executable = required_string(obj, keys::EXECUTABLE)?;
    if executable.trim().is_empty() {
        return Err(ConfError::new(format!(
            "'{}.{}' must be set (no implicit git binary)",
            keys::TOOL_GIT,
            keys::EXECUTABLE
        )));
    }
    let execution_limit = required_u64(obj, keys::EXECUTION_LIMIT)?;
    Ok(GitSettings {
        executable,
        execution_limit,
    })
}

fn load_terminal(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<TerminalSettings, ConfError> {
    let cwd_list = optional_string_list(obj, keys::CWD_LIST)?
        .iter()
        .map(|item| resolve_in_cwd(item, cwd).map(|path| path.to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(TerminalSettings {
        cwd_list,
        available_commands: optional_string_list(obj, keys::AVAILABLE_COMMANDS)?,
        command_execution_limit: required_u64(obj, keys::COMMAND_EXECUTION_LIMIT)?,
    })
}

fn load_skills(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<SkillsSettings, ConfError> {
    let items = load_named_files(obj, cwd, "skill")?
        .into_iter()
        .map(|(name, (description, file))| (name, SkillEntry { description, file }))
        .collect();
    Ok(SkillsSettings { items })
}

fn load_roles(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<RolesSettings, ConfError> {
    let items = load_named_files(obj, cwd, "role")?
        .into_iter()
        .map(|(name, (description, file))| (name, RoleEntry { description, file }))
        .collect();
    Ok(RolesSettings { items })
}

fn load_named_files(
    obj: &serde_json::Map<String, Value>,
    cwd: &Path,
    kind: &str,
) -> Result<BTreeMap<String, (String, PathBuf)>, ConfError> {
    let mut items = BTreeMap::new();
    for (name, value) in obj {
        if !is_catalog_name(name) {
            return Err(ConfError::new(format!("invalid {kind} name '{name}'")));
        }
        let entry = expect_object(value, name)?;
        for key in entry.keys() {
            if key != keys::DESCRIPTION && key != keys::FILE {
                return Err(ConfError::new(format!(
                    "unknown key '{key}' in {kind} '{name}'"
                )));
            }
        }
        let description = required_string(entry, keys::DESCRIPTION)?;
        if description.is_empty() {
            return Err(ConfError::new(format!(
                "'{}' must be a non-empty string",
                keys::DESCRIPTION
            )));
        }
        let file_raw = required_string(entry, keys::FILE)?;
        if file_raw.is_empty() {
            return Err(ConfError::new(format!(
                "'{}' must be a non-empty string",
                keys::FILE
            )));
        }
        let file = resolve_in_cwd(&file_raw, cwd)?;
        if !file.is_file() {
            return Err(ConfError::new(format!(
                "error → {kind}-file '{}' doesn't find",
                file.display()
            )));
        }
        items.insert(name.clone(), (description, file));
    }
    Ok(items)
}

fn is_catalog_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn log_folder_from_agent(
    agent_obj: &serde_json::Map<String, Value>,
    cwd: &Path,
) -> Result<String, ConfError> {
    let folder = match agent_obj.get(keys::LOG_FOLDER) {
        Some(v) => {
            let s = expect_string(v, keys::LOG_FOLDER)?;
            if s.is_empty() {
                return Err(ConfError::new(format!(
                    "'{}' must be a non-empty string",
                    keys::LOG_FOLDER
                )));
            }
            s
        }
        None => keys::DEFAULT_LOG_FOLDER.to_string(),
    };
    resolve_in_cwd(&folder, cwd)?;
    Ok(folder)
}

fn expect_object<'a>(
    value: &'a Value,
    what: &str,
) -> Result<&'a serde_json::Map<String, Value>, ConfError> {
    value
        .as_object()
        .ok_or_else(|| ConfError::new(format!("'{what}' must be an object")))
}

fn expect_string(value: &Value, key: &str) -> Result<String, ConfError> {
    value
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ConfError::new(format!("'{key}' must be a string")))
}

fn required_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, ConfError> {
    match obj.get(key) {
        Some(v) => expect_string(v, key),
        None => Err(ConfError::new(format!("missing '{key}'"))),
    }
}

fn required_i64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<i64, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in the conf file"
        ))),
        Some(v) => as_i64(v).ok_or_else(|| ConfError::new(format!("'{key}' must be a number"))),
    }
}

fn required_f64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<f64, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in the conf file"
        ))),
        Some(v) => as_f64(v).ok_or_else(|| ConfError::new(format!("'{key}' must be a number"))),
    }
}

fn required_u64(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u64, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in the conf file"
        ))),
        Some(v) => as_u64(v).ok_or_else(|| ConfError::new(format!("'{key}' must be a number"))),
    }
}

fn optional_string_list(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ConfError> {
    match obj.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(|v| expect_string(v, key)).collect(),
        Some(_) => Err(ConfError::new(format!(
            "'{key}' must be an array of strings"
        ))),
    }
}

fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_f64().map(|n| n as i64))
}

fn as_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value
                .as_f64()
                .and_then(|n| if n >= 0.0 { Some(n as u64) } else { None })
        })
}

fn strict_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_i64().and_then(|i| u64::try_from(i).ok())),
        _ => None,
    }
}

fn required_string_list(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in '{}.{}' (it may be an empty list)",
            keys::TOOLS,
            keys::TOOL_FS
        ))),
        Some(Value::Array(items)) => items.iter().map(|v| expect_string(v, key)).collect(),
        Some(_) => Err(ConfError::new(format!(
            "'{key}' must be an array of strings"
        ))),
    }
}

fn required_deprecated_list(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Vec<String>, ConfError> {
    let items = required_string_list(obj, key)?;
    for item in &items {
        if item.is_empty() {
            return Err(ConfError::new(format!(
                "'{key}' must not contain empty strings"
            )));
        }
        if item.starts_with("r:") || item.starts_with("w:") {
            return Err(ConfError::new(format!(
                "'{key}' entries must not use 'r:' / 'w:' prefixes (those mark access mode and are meaningless for a denylist): '{item}'"
            )));
        }
    }
    Ok(items)
}

fn required_model_int(obj: &serde_json::Map<String, Value>, key: &str) -> Result<i64, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in the model conf file"
        ))),
        Some(v) => match strict_i64(v) {
            Some(n) if n == -1 || n >= 1 => Ok(n),
            _ => Err(ConfError::new(format!(
                "'{key}' must be -1 or a positive integer"
            ))),
        },
    }
}

fn strict_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_u64().and_then(|u| i64::try_from(u).ok())),
        _ => None,
    }
}

/// `limits` / `price` fields a directory listing needs. `limits.USD` is not included.
pub(crate) struct ModelCandidateFields {
    pub token_limit: i64,
    pub rpm: i64,
    pub tpm: i64,
    pub rpd: i64,
    pub input_price: f64,
    pub output_price: f64,
    pub cached_price: f64,
}

/// Validate a model object for scanning. Does not read `api-key` or `limits.USD`.
pub(crate) fn model_candidate_fields(
    model_obj: &serde_json::Map<String, Value>,
) -> Result<ModelCandidateFields, ConfError> {
    let numbers = read_model_numbers(model_obj, false)?;
    Ok(ModelCandidateFields {
        token_limit: numbers.token_limit,
        rpm: numbers.rpm,
        tpm: numbers.tpm,
        rpd: numbers.rpd,
        input_price: numbers.input_price,
        output_price: numbers.output_price,
        cached_price: numbers.cached_price,
    })
}

struct ModelNumbers {
    token_limit: i64,
    /// Valid only when `require_usd` was true. Otherwise unused.
    usd_limit: f64,
    rpm: i64,
    tpm: i64,
    rpd: i64,
    input_price: f64,
    output_price: f64,
    cached_price: f64,
}

fn read_model_numbers(
    model_obj: &serde_json::Map<String, Value>,
    require_usd: bool,
) -> Result<ModelNumbers, ConfError> {
    reject_legacy_model_keys(model_obj)?;
    let limits = required_model_object(model_obj, keys::LIMITS)?;
    reject_unknown_keys(
        limits,
        &[
            keys::LIMIT_TOKENS,
            keys::LIMIT_USD,
            keys::RPM,
            keys::TPM,
            keys::RPD,
        ],
        keys::LIMITS,
    )?;
    let token_limit = required_model_int(limits, keys::LIMIT_TOKENS)?;
    let usd_limit = if require_usd {
        required_model_non_negative(limits, keys::LIMIT_USD)?
    } else {
        0.0
    };
    let rpm = required_model_int(limits, keys::RPM)?;
    let tpm = required_model_int(limits, keys::TPM)?;
    let rpd = required_model_int(limits, keys::RPD)?;
    let price = read_price(model_obj)?;
    Ok(ModelNumbers {
        token_limit,
        usd_limit,
        rpm,
        tpm,
        rpd,
        input_price: price.input,
        output_price: price.output,
        cached_price: price.cached,
    })
}

struct ModelPrice {
    input: f64,
    output: f64,
    cached: f64,
}

fn read_price(model_obj: &serde_json::Map<String, Value>) -> Result<ModelPrice, ConfError> {
    let price = required_model_object(model_obj, keys::PRICE)?;
    reject_unknown_keys(
        price,
        &[keys::INPUT_PRICE, keys::OUTPUT_PRICE, keys::CACHED_PRICE],
        keys::PRICE,
    )?;
    Ok(ModelPrice {
        input: required_model_non_negative(price, keys::INPUT_PRICE)?,
        output: required_model_non_negative(price, keys::OUTPUT_PRICE)?,
        cached: required_model_non_negative(price, keys::CACHED_PRICE)?,
    })
}

fn reject_legacy_model_keys(obj: &serde_json::Map<String, Value>) -> Result<(), ConfError> {
    const LEGACY: &[&str] = &[
        keys::TOKEN_LIMIT,
        keys::ITERATION_LIMIT,
        keys::ITERATIONS,
        keys::USD_LIMIT,
        keys::RPM,
        keys::TPM,
        keys::RPD,
    ];
    for key in LEGACY {
        if obj.contains_key(*key) {
            return Err(ConfError::new(format!(
                "'{key}' is not allowed in the model file"
            )));
        }
    }
    Ok(())
}

fn reject_unknown_keys(
    obj: &serde_json::Map<String, Value>,
    allowed: &[&str],
    parent: &str,
) -> Result<(), ConfError> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ConfError::new(format!("unknown key '{key}' in '{parent}'")));
        }
    }
    Ok(())
}

fn required_model_object<'a>(
    obj: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a serde_json::Map<String, Value>, ConfError> {
    match obj.get(key) {
        Some(Value::Object(map)) => Ok(map),
        _ => Err(ConfError::new(format!(
            "please define '{key}' in the model conf file"
        ))),
    }
}

fn required_model_non_negative(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<f64, ConfError> {
    match obj.get(key) {
        None => Err(ConfError::new(format!(
            "please define '{key}' in the model conf file"
        ))),
        Some(v) => match strict_f64(v) {
            Some(n) if n.is_finite() && (n == -1.0 || n >= 0.0) => Ok(n),
            _ => Err(ConfError::new(format!(
                "'{key}' must be -1 or a non-negative number"
            ))),
        },
    }
}

fn strict_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}
