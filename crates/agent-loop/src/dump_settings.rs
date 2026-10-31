//! Embed harness settings into vendor dump files, and extract them on resume.
//!
//! Vendor agents write only their own fields. This module adds/reads the
//! top-level `"settings"` object so a session can continue without `--conf`.

use std::path::{Path, PathBuf};

use conf::str::keys;
use serde_json::Value;

/// Top-level dump key holding the conf JSON (after vars, with absolute `agent.config`).
pub const SETTINGS_KEY: &str = "settings";

/// Insert or replace `"settings"` in a dump JSON file, keeping vendor fields.
pub fn embed(path: &Path, settings: &Value) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read dump '{}': {e}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse dump '{}': {e}", path.display()))?;
    let mut out = serde_json::Map::new();
    out.insert(SETTINGS_KEY.to_string(), settings.clone());
    match root {
        Value::Object(map) => {
            for (k, v) in map {
                if k != SETTINGS_KEY {
                    out.insert(k, v);
                }
            }
        }
        _ => {
            return Err(format!("dump '{}' is not a JSON object", path.display()));
        }
    }
    let text = serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())?;
    std::fs::write(path, text)
        .map_err(|e| format!("cannot write dump '{}': {e}", path.display()))?;
    Ok(())
}

/// Read the `"settings"` object from a dump file.
pub fn extract(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read dump '{}': {e}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse dump '{}': {e}", path.display()))?;
    match root.get(SETTINGS_KEY) {
        Some(v) if v.is_object() => Ok(v.clone()),
        _ => Err(missing_settings_error(path)),
    }
}

/// Error when a dump has no embedded configuration.
pub fn missing_settings_error(path: &Path) -> String {
    format!(
        "dump '{}' has no settings; cannot continue without embedded configuration",
        path.display()
    )
}

/// Directory used as `conf_dir` when loading dump settings: parent of `agent.config`.
pub fn conf_dir_of(settings: &Value) -> Result<PathBuf, String> {
    let config = settings
        .get(keys::AGENT)
        .and_then(|a| a.get(keys::CONFIG))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "dump settings missing agent.config".to_string())?;
    let path = PathBuf::from(config);
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        _ => Ok(PathBuf::from(".")),
    }
}
