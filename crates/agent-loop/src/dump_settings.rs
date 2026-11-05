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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dump_path(name: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "harness-dump-settings-{}-{id}-{name}.json",
            std::process::id()
        ))
    }

    #[test]
    fn embed_writes_settings_and_cwd_and_extract_reads_them_back() {
        let path = temp_dump_path("embed");
        std::fs::write(&path, r#"{"vendor": "keep-me"}"#).unwrap();

        let cwd = std::env::current_dir().unwrap().join("work");
        let settings = serde_json::json!({
            "agent": { "config": cwd.join("agent.json5").to_string_lossy().into_owned() },
            "tools": {}
        });
        embed(&path, &settings, &cwd).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let root: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(root.get("vendor").and_then(Value::as_str), Some("keep-me"));
        assert_eq!(
            root.get(CWD_KEY).and_then(Value::as_str),
            Some(cwd.to_string_lossy().as_ref())
        );
        assert_eq!(root.get(SETTINGS_KEY), Some(&settings));
        assert!(settings.get("cwd").is_none());

        let (extracted_settings, extracted_cwd) = extract(&path).unwrap();
        assert_eq!(extracted_settings, settings);
        assert_eq!(extracted_cwd, cwd);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn extract_rejects_dump_without_cwd() {
        let path = temp_dump_path("no-cwd");
        std::fs::write(&path, r#"{"settings": {"agent": {}}}"#).unwrap();

        let error = extract(&path).unwrap_err();
        assert!(error.contains("has no CWD"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn extract_rejects_dump_with_non_absolute_cwd() {
        let path = temp_dump_path("relative-cwd");
        std::fs::write(&path, r#"{"settings": {"agent": {}}, "CWD": "relative/path"}"#)
            .unwrap();

        let error = extract(&path).unwrap_err();
        assert!(error.contains("non-absolute CWD"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn embed_extract_round_trip_with_real_conf_source() {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let cwd = std::env::temp_dir().join(format!(
            "harness-dump-settings-conf-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&cwd).unwrap();

        let conf_path = cwd.join("harness.json5");
        let agent_config_path = cwd.join("agent.config");
        let dump_path = cwd.join("session.dump.json");

        std::fs::write(&agent_config_path, "").unwrap();
        std::fs::write(
            &conf_path,
            r#"{
                agent: {
                    config: "agent.config",
                    mode: "safe",
                    "token-limit": 1000,
                    "iteration-limit": 10,
                    "USD-limit": 5,
                }
            }"#,
        )
        .unwrap();
        std::fs::write(&dump_path, r#"{"vendor": "keep-me"}"#).unwrap();

        let settings = match conf::load_server_config(&conf_path, &cwd).unwrap() {
            conf::ServerConfigState::Ok(settings) => settings,
            other => panic!("expected Ok(settings), got {other:?}"),
        };
        assert!(settings.source.get("cwd").is_none());

        embed(&dump_path, &settings.source, &cwd).unwrap();
        let (extracted_settings, extracted_cwd) = extract(&dump_path).unwrap();

        assert!(extracted_settings.get("cwd").is_none());
        assert_eq!(extracted_settings, settings.source);
        assert!(extracted_cwd.is_absolute());
        assert_eq!(extracted_cwd, cwd);

        let _ = std::fs::remove_dir_all(&cwd);
    }
}
