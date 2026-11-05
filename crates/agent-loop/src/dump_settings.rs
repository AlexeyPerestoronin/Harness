//! Embed harness settings into vendor dump files, and extract them on resume.
//!
//! Vendor agents write only their own fields. This module adds/reads the
//! top-level `"settings"` object and the top-level `"CWD"` value so a session
//! can continue without `--conf` and without `--cwd`.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Top-level dump key holding the conf JSON (after vars, with absolute `agent.config`).
pub const SETTINGS_KEY: &str = "settings";
/// Top-level dump key holding the absolute normalized CWD of the session.
pub const CWD_KEY: &str = "CWD";

/// Insert or replace `"settings"` and `"CWD"` in a dump JSON file, keeping vendor fields.
pub fn embed(path: &Path, settings: &Value, cwd: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read dump '{}': {e}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse dump '{}': {e}", path.display()))?;
    let mut out = serde_json::Map::new();
    out.insert(SETTINGS_KEY.to_string(), settings.clone());
    out.insert(
        CWD_KEY.to_string(),
        Value::String(cwd.to_string_lossy().into_owned()),
    );
    match root {
        Value::Object(map) => {
            for (k, v) in map {
                if k != SETTINGS_KEY && k != CWD_KEY {
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

/// Read the `"settings"` object and the `"CWD"` value from a dump file.
pub fn extract(path: &Path) -> Result<(Value, PathBuf), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read dump '{}': {e}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|e| format!("cannot parse dump '{}': {e}", path.display()))?;
    let settings = match root.get(SETTINGS_KEY) {
        Some(v) if v.is_object() => v.clone(),
        _ => return Err(missing_settings_error(path)),
    };
    let cwd = match root.get(CWD_KEY) {
        Some(Value::String(raw)) => {
            let cwd = PathBuf::from(raw);
            if cwd.is_absolute() {
                cwd
            } else {
                return Err(format!(
                    "dump '{}' has a non-absolute CWD '{}'; cannot continue",
                    path.display(),
                    cwd.display()
                ));
            }
        }
        Some(_) => {
            return Err(format!(
                "dump '{}' has an invalid CWD value; expected an absolute path string",
                path.display()
            ))
        }
        None => {
            return Err(format!(
                "dump '{}' has no CWD; cannot continue without CWD",
                path.display()
            ))
        }
    };
    Ok((settings, cwd))
}

/// Error when a dump has no embedded configuration.
pub fn missing_settings_error(path: &Path) -> String {
    format!(
        "dump '{}' has no settings; cannot continue without embedded configuration",
        path.display()
    )
}
