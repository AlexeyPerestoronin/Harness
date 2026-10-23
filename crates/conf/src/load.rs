//! Load `harness.json5` and the nested model file into [`crate::Settings`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::ConfError;
use crate::scan::{scan_agent_dir, SkippedFile};
use crate::str::keys;
use crate::types::{
    AgentCandidate, AgentKind, AgentSettings, FsSettings, GitSettings, Mode, Settings,
    TerminalSettings, ToolsSettings, UserSettings,
};
use serde_json::Value;

/// Load and validate a harness configuration file.
///
/// If `agent.config` is a directory this fails with
/// `agent was not selected`. Use [`load_with`] to choose a candidate.
pub fn load(conf_path: &Path) -> Result<Settings, ConfError> {
    load_with(conf_path, |_| Err("agent was not selected".into()))
}

/// Load and validate a harness configuration file.
///
/// When `agent.config` is a **file**, `chooser` is not called.
/// When it is a **directory**, `chooser` receives the sorted candidate list
/// and must return a 0-based index. Session limits (`mode`, `token-limit`,
/// `iteration-limit`, `USD-limit`) always come from `harness.json5`.
pub fn load_with<F>(conf_path: &Path, chooser: F) -> Result<Settings, ConfError>
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

    load_from_value(root, &conf_dir, chooser)
}

/// Validate a conf JSON object (already after vars substitution) into [`Settings`].
///
/// `vars` is not substituted again. `conf_dir` resolves a relative `agent.config`.
/// After a successful load, [`Settings::source`] is `root` with `agent.config`
/// rewritten to the absolute chosen agent file.
pub fn load_from_value<F>(
    mut root: Value,
    conf_dir: &Path,
    mut chooser: F,
) -> Result<Settings, ConfError>
where
    F: FnMut(&[AgentCandidate]) -> Result<usize, String>,
{
    let (cwd, log_folder, agent, tools) = {
        let obj = expect_object(&root, "settings")?;
        let cwd = required_string(obj, keys::CWD)?;
        if cwd.trim().is_empty() {
            return Err(ConfError::new(format!(
                "'{}' must be set explicitly",
                keys::CWD
            )));
        }

        let agent_raw = obj
            .get(keys::AGENT)
            .ok_or_else(|| ConfError::new(format!("missing '{}'", keys::AGENT)))?;
        let agent_obj = expect_object(agent_raw, keys::AGENT)?;
        let log_folder = log_folder_from_agent(agent_obj)?;
        let agent = load_agent(agent_obj, conf_dir, &mut chooser)?;

        let tools = match obj.get(keys::TOOLS) {
            Some(v) => load_tools(expect_object(v, keys::TOOLS)?)?,
            None => ToolsSettings::default(),
        };
        (cwd, log_folder, agent, tools)
    };

    rewrite_agent_config(&mut root, &agent.config_path);

    Ok(Settings {
        cwd: PathBuf::from(cwd),
        conf_dir: conf_dir.to_path_buf(),
        log_folder,
        agent,
        tools,
        source: root,
    })
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
/// Session limits (`token-limit`, `iteration-limit`, `USD-limit`) are read from
/// the model file (unlike [`load`], which takes them from `harness.json5`).
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
    limits: Option<(i64, i64, f64)>,
    config_path: PathBuf,
) -> Result<AgentSettings, ConfError> {
    let vendor = required_string(model_obj, keys::VENDOR)?;
    let name = required_string(model_obj, keys::NAME)?;
    let api_key = resolve_api_key(model_obj)?;
    let kind = AgentKind::from_vendor_name(&vendor, &name)?;

    let rpm = optional_u32(model_obj, keys::RPM)?;
    let tpm = optional_u32(model_obj, keys::TPM)?;
    let rpd = optional_u32(model_obj, keys::RPD)?;

    if matches!(kind, AgentKind::Google { .. }) && rpm.is_none() {
        return Err(ConfError::new(format!(
            "model '{name}' requires '{rpm}' in the model conf file",
            rpm = keys::RPM
        )));
    }

    let (token_limit, iteration_limit, usd_limit) = match limits {
        Some(v) => v,
        None => (
            required_i64(model_obj, keys::TOKEN_LIMIT)?,
            required_i64(model_obj, keys::ITERATION_LIMIT)?,
            required_f64(model_obj, keys::USD_LIMIT)?,
        ),
    };

    Ok(AgentSettings {
        vendor,
        name,
        api_key,
        kind,
        mode,
        token_limit,
        iteration_limit,
        usd_limit,
        rpm,
        tpm,
        rpd,
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
        return Err(ConfError::new(format!(
            "please set '{}' in the model file",
            keys::API_KEY
        )));
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

fn load_tools(obj: &serde_json::Map<String, Value>) -> Result<ToolsSettings, ConfError> {
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
        invoke_execution_limit: required_u64(obj, keys::INVOKE_EXECUTION_LIMIT)?,
    })
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

fn optional_u32(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Option<u32>, ConfError> {
    match obj.get(key) {
        None => Ok(None),
        Some(v) => as_u64(v)
            .map(|n| Some(n as u32))
            .ok_or_else(|| ConfError::new(format!("'{key}' must be a number"))),
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
