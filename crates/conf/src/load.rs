//! Load `harness.json5` and the nested model file into [`crate::Settings`].
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
    AgentCandidate, AgentKind, AgentSettings, FsSettings, GitSettings, Mode, RoleEntry,
    RolesSettings, Settings, SkillEntry, SkillsSettings, TerminalSettings, ToolsSettings,
    UserSettings,
};
use serde_json::Value;

/// Load and validate a harness configuration file.
///
/// If `agent.config` is a directory this fails with
/// `agent was not selected`. Use [`load_with`] to choose a candidate.
pub fn load(conf_path: &Path) -> Result<Settings, ConfError> {
    load_with(conf_path, None, |_| Err("agent was not selected".into()))
}

/// Load and validate a harness configuration file.
///
/// When `agent.config` is a **file**, `chooser` is not called.
/// When it is a **directory**, `chooser` receives the sorted candidate list
/// and must return a 0-based index. Session limits (`mode`, `token-limit`,
/// `iteration-limit`, `USD-limit`) always come from `harness.json5`.
/// `cwd_override` replaces `cwd` from the file (used by `--cwd`).
pub fn load_with<F>(
    conf_path: &Path,
    cwd_override: Option<PathBuf>,
    chooser: F,
) -> Result<Settings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let conf_path = if conf_path.is_absolute() {
        conf_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(conf_path)
    };
    let text = fs::read_to_string(&conf_path).map_err(|e| {
        ConfError::new(format!(
            "cannot read conf file '{}': {e}",
            conf_path.display()
        ))
    })?;
    let text = crate::vars::substitute(&text, &conf_path.display().to_string())?;
    let root: Value = json5::from_str(&text)?;
    expect_object(&root, &conf_path.display().to_string())?;
    let conf_dir = conf_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    load_from_value(root, &conf_dir, cwd_override, chooser)
}

/// Validate a conf JSON object (already after vars substitution) into [`Settings`].
///
/// `vars` is not substituted again. `conf_dir` resolves a relative `agent.config`.
/// `cwd_override` replaces `cwd` from `root`; relative paths use `conf_dir`.
/// After a successful load, [`Settings::source`] is `root` with `agent.config`
/// rewritten to the absolute chosen agent file.
pub fn load_from_value<F>(
    mut root: Value,
    conf_dir: &Path,
    cwd_override: Option<PathBuf>,
    mut chooser: F,
) -> Result<Settings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let (cwd, log_folder, agent, tools) = {
        let obj = expect_object(&root, "settings")?;
        let cwd = match cwd_override {
            Some(raw) => resolve_cwd_path(&raw, conf_dir)?,
            None => {
                let raw = required_string(obj, keys::CWD)?;
                resolve_cwd(&raw, conf_dir)?
            }
        };

        let agent_raw = obj
            .get(keys::AGENT)
            .ok_or_else(|| ConfError::new(format!("missing '{}'", keys::AGENT)))?;
        let agent_obj = expect_object(agent_raw, keys::AGENT)?;
        let log_folder = log_folder_from_agent(agent_obj)?;
        let agent = load_agent(agent_obj, conf_dir, &mut chooser)?;

        let tools = match obj.get(keys::TOOLS) {
            Some(v) => load_tools(expect_object(v, keys::TOOLS)?, conf_dir)?,
            None => ToolsSettings::default(),
        };
        (cwd, log_folder, agent, tools)
    };

    rewrite_agent_config(&mut root, &agent.config_path);

    Ok(Settings {
        cwd,
        conf_dir: conf_dir.to_path_buf(),
        log_folder,
        agent,
        tools,
        source: root,
    })
}

fn resolve_cwd(raw: &str, conf_dir: &Path) -> Result<PathBuf, ConfError> {
    if raw.trim().is_empty() {
        return Err(ConfError::new(format!(
            "'{}' must be set explicitly",
            keys::CWD
        )));
    }
    resolve_cwd_path(Path::new(raw), conf_dir)
}

fn resolve_cwd_path(raw: &Path, conf_dir: &Path) -> Result<PathBuf, ConfError> {
    if raw.as_os_str().is_empty() {
        return Err(ConfError::new(format!(
            "'{}' must be set explicitly",
            keys::CWD
        )));
    }
    let base = if conf_dir.is_absolute() {
        conf_dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(conf_dir)
    };
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };
    normalize_abs(&absolute)
}

fn normalize_abs(path: &Path) -> Result<PathBuf, ConfError> {
    use std::path::Component;

    if !path.is_absolute() {
        return Err(ConfError::new(format!(
            "'{}' must be an absolute path",
            keys::CWD
        )));
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
        return Err(ConfError::new(format!(
            "'{}' must be set explicitly",
            keys::CWD
        )));
    }
    Ok(normalized)
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

/// Load a model json5 file into [`AgentSettings`].
///
/// `mode` is not a model-file field; the caller supplies it.
/// `token_limit` and `usd_limit` come from `limits`. `iteration_limit` is
/// always `-1`: session iterations live only in `harness.json5`.
pub fn load_model(model_path: &Path, mode: Mode) -> Result<AgentSettings, ConfError> {
    let model_path = if model_path.is_absolute() {
        model_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(model_path)
    };
    let model_obj = read_model_object(&model_path)?;
    settings_from_model(&model_obj, mode, None, model_path)
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
    conf_dir: &Path,
    chooser: &mut F,
) -> Result<AgentSettings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let config_rel = required_string(agent_obj, keys::CONFIG)?;
    let mode_raw = required_string(agent_obj, keys::MODE)?;
    let mode = match mode_raw.as_str() {
        v if v == keys::MODE_SAFE => Mode::Safe,
        v if v == keys::MODE_AUTO_ACCEPT => Mode::AutoAccept,
        other => {
            return Err(ConfError::new(format!(
                "invalid '{}': '{other}' (expected '{}' or '{}')",
                keys::MODE,
                keys::MODE_SAFE,
                keys::MODE_AUTO_ACCEPT
            )))
        }
    };

    let token_limit = required_i64(agent_obj, keys::TOKEN_LIMIT)?;
    let iteration_limit = required_i64(agent_obj, keys::ITERATION_LIMIT)?;
    let usd_limit = required_f64(agent_obj, keys::USD_LIMIT)?;

    let model_path = {
        let p = PathBuf::from(&config_rel);
        if p.is_absolute() {
            p
        } else {
            conf_dir.join(p)
        }
    };
    let model_path = if model_path.is_dir() {
        choose_from_dir(&model_path, chooser)?
    } else {
        model_path
    };
    let model_path = if model_path.is_absolute() {
        model_path
    } else {
        std::env::current_dir()?.join(model_path)
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
    )
}

fn choose_from_dir<F>(dir: &Path, chooser: &mut F) -> Result<PathBuf, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let (candidates, skipped) = scan_agent_dir(dir)?;
    report_skipped(&skipped);
    if candidates.is_empty() {
        let mut msg = format!("no agent config files found in '{}'", dir.display());
        for s in &skipped {
            msg.push_str(&format!("\nskipped '{}': {}", s.file, s.reason));
        }
        return Err(ConfError::new(msg));
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
) -> Result<AgentSettings, ConfError> {
    let numbers = read_model_numbers(model_obj, true)?;
    let vendor = required_string(model_obj, keys::VENDOR)?;
    let name = required_string(model_obj, keys::NAME)?;
    let api_key = resolve_api_key(model_obj)?;
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

fn resolve_api_key(model_obj: &serde_json::Map<String, Value>) -> Result<String, ConfError> {
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
    let candidate = PathBuf::from(trimmed);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        std::env::current_dir()?.join(candidate)
    };
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
    conf_dir: &Path,
) -> Result<ToolsSettings, ConfError> {
    let mut settings = ToolsSettings::default();
    for (name, value) in obj {
        let module = expect_object(value, name)?;
        if name == keys::TOOL_FS {
            settings.fs = Some(load_fs(module)?);
        } else if name == keys::TOOL_GIT {
            settings.git = Some(load_git(module)?);
        } else if name == keys::TOOL_TERMINAL {
            settings.terminal = Some(load_terminal(module)?);
        } else if name == keys::TOOL_USER {
            settings.user = Some(UserSettings {});
        } else if name == keys::TOOL_SKILLS {
            settings.skills = Some(load_skills(module, conf_dir)?);
        } else if name == keys::TOOL_ROLES {
            settings.roles = Some(load_roles(module, conf_dir)?);
        } else {
            return Err(ConfError::new(format!("unknown tool module '{name}'")));
        }
    }
    Ok(settings)
}

fn load_fs(obj: &serde_json::Map<String, Value>) -> Result<FsSettings, ConfError> {
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
    crate::fs_scope::check_conflicts(
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

fn load_terminal(obj: &serde_json::Map<String, Value>) -> Result<TerminalSettings, ConfError> {
    Ok(TerminalSettings {
        cwd_list: optional_string_list(obj, keys::CWD_LIST)?,
        available_commands: optional_string_list(obj, keys::AVAILABLE_COMMANDS)?,
        command_execution_limit: required_u64(obj, keys::COMMAND_EXECUTION_LIMIT)?,
    })
}

fn load_skills(
    obj: &serde_json::Map<String, Value>,
    conf_dir: &Path,
) -> Result<SkillsSettings, ConfError> {
    let items = load_named_files(obj, conf_dir, "skill")?
        .into_iter()
        .map(|(name, (description, file))| (name, SkillEntry { description, file }))
        .collect();
    Ok(SkillsSettings { items })
}

fn load_roles(
    obj: &serde_json::Map<String, Value>,
    conf_dir: &Path,
) -> Result<RolesSettings, ConfError> {
    let items = load_named_files(obj, conf_dir, "role")?
        .into_iter()
        .map(|(name, (description, file))| (name, RoleEntry { description, file }))
        .collect();
    Ok(RolesSettings { items })
}

fn load_named_files(
    obj: &serde_json::Map<String, Value>,
    conf_dir: &Path,
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
        let file = resolve_conf_path(conf_dir, &file_raw)?;
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

fn resolve_conf_path(conf_dir: &Path, raw: &str) -> Result<PathBuf, ConfError> {
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        conf_dir.join(path)
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn log_folder_from_agent(agent_obj: &serde_json::Map<String, Value>) -> Result<String, ConfError> {
    match agent_obj.get(keys::LOG_FOLDER) {
        Some(v) => {
            let s = expect_string(v, keys::LOG_FOLDER)?;
            if s.is_empty() {
                return Err(ConfError::new(format!(
                    "'{}' must be a non-empty string",
                    keys::LOG_FOLDER
                )));
            }
            Ok(s)
        }
        None => Ok(keys::DEFAULT_LOG_FOLDER.to_string()),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn model_obj_with(key: &str, value: Value) -> serde_json::Map<String, Value> {
        let mut obj = serde_json::Map::new();
        obj.insert(key.to_string(), value);
        obj
    }

    #[test]
    fn resolve_api_key_allows_empty_and_whitespace() {
        assert_eq!(
            resolve_api_key(&model_obj_with(keys::API_KEY, Value::String(String::new()))).unwrap(),
            ""
        );
        assert_eq!(
            resolve_api_key(&model_obj_with(
                keys::API_KEY,
                Value::String("   ".to_string())
            ))
            .unwrap(),
            ""
        );
    }

    #[test]
    fn resolve_api_key_requires_key() {
        assert!(resolve_api_key(&serde_json::Map::new()).is_err());
    }

    #[test]
    fn relative_cwd_resolves_against_conf_dir() {
        let conf_dir = std::env::current_dir().unwrap().join("proj").join("conf");
        assert_eq!(resolve_cwd(".", &conf_dir).unwrap(), conf_dir);
        assert_eq!(
            resolve_cwd("..", &conf_dir).unwrap(),
            conf_dir.parent().unwrap().to_path_buf()
        );
    }

    #[test]
    fn absolute_cwd_is_normalized() {
        let conf_dir = std::env::current_dir().unwrap();
        let raw = conf_dir.join("a").join("..").join("b");
        assert_eq!(
            resolve_cwd(&raw.to_string_lossy(), &conf_dir).unwrap(),
            conf_dir.join("b")
        );
    }
}
