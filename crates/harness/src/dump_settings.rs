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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;

    use crate::agents::{Agent, ClaudeAgent, GoogleAgent, XaiAgent};
    use conf::{AgentKind, AgentSettings, Mode, ToolsSettings};
    use logger::{Logger, NullLogger};
    use serde_json::json;
    use tools::{ToolRegistry, ToolsContext};

    fn unique_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "harness-dump-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_settings() -> Value {
        json!({
            "cwd": "/ws",
            "agent": {
                "config": "/abs/agent.json5",
                "mode": "safe",
                "token-limit": 100,
                "iteration-limit": 10,
                "USD-limit": 1.5
            },
            "tools": {}
        })
    }

    fn write_dump(dir: &Path, name: &str, body: &Value) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, serde_json::to_string_pretty(body).unwrap()).unwrap();
        path
    }

    fn assert_vendor_roundtrip(name: &str, dump: Value) {
        let dir = unique_dir();
        let path = write_dump(&dir, name, &dump);
        let settings = sample_settings();
        embed(&path, &settings).unwrap();
        let root: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["settings"]["agent"]["config"], "/abs/agent.json5");
        for (k, v) in dump.as_object().unwrap() {
            assert_eq!(&root[k], v, "lost vendor field {k}");
        }
        let extracted = extract(&path).unwrap();
        assert_eq!(extracted["agent"]["config"], "/abs/agent.json5");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn embed_preserves_xai_fields() {
        assert_vendor_roundtrip(
            "xai.json",
            json!({
                "conv_id": "conv",
                "chat_id": "chat",
                "usage_stats": {
                    "requests": 1,
                    "input_tokens": 2,
                    "output_tokens": 3,
                    "cached_tokens": 0,
                    "total_cost_usd": 0.1
                },
                "messages": [{"role": "user", "content": "hi"}]
            }),
        );
    }

    #[test]
    fn embed_preserves_google_fields() {
        assert_vendor_roundtrip(
            "google.json",
            json!({
                "usage_stats": {
                    "requests": 1,
                    "input_tokens": 2,
                    "output_tokens": 3,
                    "cached_tokens": 0,
                    "total_cost_usd": 0.0
                },
                "history": [{"role": "user", "parts": [{"text": "hi"}]}],
                "pending_message": null
            }),
        );
    }

    #[test]
    fn embed_preserves_claude_fields() {
        assert_vendor_roundtrip(
            "claude.json",
            json!({
                "messages": [{"role": "user", "content": "hi"}],
                "usage": {
                    "requests": 1,
                    "input_tokens": 2,
                    "output_tokens": 3,
                    "cached_tokens": 0,
                    "total_cost_usd": 0.0
                }
            }),
        );
    }

    #[test]
    fn extract_without_settings_errors() {
        let dir = unique_dir();
        let path = write_dump(&dir, "old.json", &json!({"conv_id": "c"}));
        let err = extract(&path).unwrap_err();
        assert_eq!(err, missing_settings_error(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    fn empty_registry() -> (Arc<ToolRegistry>, Arc<dyn Logger>) {
        let logger: Arc<dyn Logger> = Arc::new(NullLogger);
        let registry = Arc::new(ToolRegistry::build(
            &ToolsSettings::default(),
            ToolsContext {
                cwd: std::env::temp_dir(),
                temp_dir: std::env::temp_dir(),
                logger: logger.clone(),
            },
        ));
        (registry, logger)
    }

    fn base_agent(kind: AgentKind) -> AgentSettings {
        AgentSettings {
            vendor: "test".into(),
            name: "test".into(),
            api_key: "k".into(),
            kind,
            mode: Mode::Safe,
            token_limit: -1,
            iteration_limit: -1,
            usd_limit: -1.0,
            rpm: Some(10),
            tpm: None,
            rpd: None,
            config_path: PathBuf::from("/tmp/agent.json5"),
        }
    }

    fn reload_with_settings(agent: &mut dyn Agent, dump: &Path) {
        embed(dump, &sample_settings()).unwrap();
        agent.reload_session(dump).unwrap();
    }

    #[test]
    fn xai_reload_ignores_settings_key() {
        let (registry, logger) = empty_registry();
        let mut agent = XaiAgent::new(
            registry,
            logger,
            &base_agent(AgentKind::Xai {
                api_model: "grok-4.6".into(),
            }),
        )
        .unwrap();
        let dir = unique_dir();
        let dump = dir.join("dump.json");
        agent.dump_session(&dump).unwrap();
        reload_with_settings(&mut agent, &dump);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn google_reload_ignores_settings_key() {
        let (registry, logger) = empty_registry();
        let mut agent = GoogleAgent::new(
            registry,
            logger,
            &base_agent(AgentKind::Google {
                api_model: "gemini-3.8-flash".into(),
            }),
        )
        .unwrap();
        let dir = unique_dir();
        let dump = dir.join("dump.json");
        agent.dump_session(&dump).unwrap();
        reload_with_settings(&mut agent, &dump);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_reload_ignores_settings_key() {
        let (registry, logger) = empty_registry();
        let mut agent = ClaudeAgent::new(
            registry,
            logger,
            &base_agent(AgentKind::Claude {
                api_model: "claude-sonnet-5-5".into(),
            }),
        )
        .unwrap();
        let dir = unique_dir();
        let dump = dir.join("dump.json");
        agent.dump_session(&dump).unwrap();
        reload_with_settings(&mut agent, &dump);
        let _ = fs::remove_dir_all(&dir);
    }
}
