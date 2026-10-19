//! Load `harness.json5` and the nested model file into [`crate::Settings`].

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use crate::error::ConfError;
use crate::str::keys;
use crate::types::{
    AgentKind, AgentSettings, FsSettings, GitSettings, Mode, Settings, TerminalSettings,
    ToolsSettings, UserSettings,
};

/// Load and validate a harness configuration file.
pub fn load(conf_path: &Path) -> Result<Settings, ConfError> {
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
    let obj = expect_object(&root, &conf_path.display().to_string())?;
    let conf_dir = conf_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let cwd = required_string(obj, keys::CWD)?;
    if cwd.trim().is_empty() {
        return Err(ConfError::new(format!(
            "'{}' must be set explicitly",
            keys::CWD
        )));
    }

    let log_folder = match obj.get(keys::PATHS) {
        Some(paths) => {
            let paths_obj = expect_object(paths, keys::PATHS)?;
            match paths_obj.get(keys::LOG_FOLDER) {
                Some(v) => expect_string(v, keys::LOG_FOLDER)?,
                None => keys::DEFAULT_LOG_FOLDER.to_string(),
            }
        }
        None => keys::DEFAULT_LOG_FOLDER.to_string(),
    };

    let agent_raw = obj
        .get(keys::AGENT_SETTING)
        .ok_or_else(|| ConfError::new(format!("missing '{}'", keys::AGENT_SETTING)))?;
    let agent_obj = expect_object(agent_raw, keys::AGENT_SETTING)?;
    let agent = load_agent(agent_obj, &conf_dir)?;

    let tools = match obj.get(keys::TOOLS) {
        Some(v) => load_tools(expect_object(v, keys::TOOLS)?)?,
        None => ToolsSettings::default(),
    };

    Ok(Settings {
        cwd: PathBuf::from(cwd),
        conf_dir,
        log_folder,
        agent,
        tools,
    })
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
    settings_from_model(&model_obj, mode, None)
}

fn read_model_object(
    model_path: &Path,
) -> Result<serde_json::Map<String, Value>, ConfError> {
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

fn load_agent(agent_obj: &serde_json::Map<String, Value>, conf_dir: &Path) -> Result<AgentSettings, ConfError> {
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
    let model_obj = read_model_object(&model_path)?;
    settings_from_model(
        &model_obj,
        mode,
        Some((token_limit, iteration_limit, usd_limit)),
    )
}

fn settings_from_model(
    model_obj: &serde_json::Map<String, Value>,
    mode: Mode,
    limits: Option<(i64, i64, f64)>,
) -> Result<AgentSettings, ConfError> {
    let vendor = required_string(model_obj, keys::VENDOR)?;
    let name = required_string(model_obj, keys::NAME)?;
    let api_key = resolve_api_key(model_obj)?;
    let kind = agent_kind(&vendor, &name)?;

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
    })
}

fn agent_kind(vendor: &str, name: &str) -> Result<AgentKind, ConfError> {
    match (vendor, name) {
        ("SpaceXAI", "Grok 4.6") => Ok(AgentKind::Xai {
            api_model: "grok-4.6".into(),
        }),
        ("GoogleAI", "Gemini 3.8 Flash") => Ok(AgentKind::Google {
            api_model: "gemini-3.8-flash".into(),
        }),
        ("GoogleAI", "Gemini 3.5 Flash Lite") => Ok(AgentKind::Google {
            api_model: "gemini-3.5-flash-lite".into(),
        }),
        ("GoogleAI", "Gemini 3.1 Flash Lite") => Ok(AgentKind::Google {
            api_model: "gemini-3.1-flash-lite".into(),
        }),
        ("Claude", "Claude Sonnet 5.5") => Ok(AgentKind::Claude {
            api_model: "claude-sonnet-5-5".into(),
        }),
        _ => Err(ConfError::new(format!(
            "unknown agent: {vendor} {name}"
        ))),
    }
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
            return Err(ConfError::new(format!(
                "unknown tool module '{name}'"
            )));
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

fn expect_object<'a>(
    value: &'a Value,
    what: &str,
) -> Result<&'a serde_json::Map<String, Value>, ConfError> {
    value.as_object().ok_or_else(|| ConfError::new(format!("'{what}' must be an object")))
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
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| expect_string(v, key))
            .collect(),
        Some(_) => Err(ConfError::new(format!("'{key}' must be an array of strings"))),
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
        .or_else(|| value.as_f64().and_then(|n| if n >= 0.0 { Some(n as u64) } else { None }))
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
