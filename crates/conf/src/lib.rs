//! Configuration crate for the native harness.
//!
//! This is the only crate allowed to open `harness.json5` and nested model
//! files. Callers receive typed [`Settings`] and never parse json5 themselves.
//! All json5 keys live in [`str::keys`]. The `--conf-new` template is a file.

pub mod str;

mod error;
mod fs_scope;
mod load;
mod scan;
mod template;
mod types;
mod vars;

pub use error::ConfError;
pub use load::{load, load_model, load_with};
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentCandidate, AgentKind, AgentSettings, FsSettings, GitSettings, Mode, Settings,
    TerminalSettings, ToolsSettings, UserSettings,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::str::keys;
    use serde_json::Value;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch_dir() -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "harness-conf-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn json_path(path: &Path) -> String {
        path.display().to_string().replace('\\', "/")
    }

    fn grok_json() -> &'static str {
        r#"{
            "vendor": "SpaceXAI",
            "name": "Grok 4.6",
            "api-key": "k"
        }"#
    }

    fn write_valid_conf(dir: &Path, config_rel: &str, agent_extra: &str) -> PathBuf {
        let extra = if agent_extra.is_empty() {
            String::new()
        } else {
            format!(", {agent_extra}")
        };
        let path = dir.join("harness.json5");
        let cwd = json_path(dir);
        let text = format!(
            r#"{{
  "cwd": "{cwd}",
  "agent": {{
    "config": "{config_rel}",
    "mode": "safe",
    "token-limit": 100,
    "iteration-limit": 10,
    "USD-limit": 1{extra}
  }}
}}"#
        );
        fs::write(&path, text).unwrap();
        path
    }

    fn load_raw(text: &str) -> Result<Settings, ConfError> {
        let dir = scratch_dir();
        let path = dir.join("harness.json5");
        fs::write(&path, text).unwrap();
        let result = load(&path);
        let _ = fs::remove_dir_all(&dir);
        result
    }

    #[test]
    fn log_folder_from_agent_is_loaded() {
        let dir = scratch_dir();
        fs::write(dir.join("grok.json5"), grok_json()).unwrap();
        let conf = write_valid_conf(&dir, "grok.json5", "\"log-folder\": \"mylogs\"");
        let settings = load(&conf).unwrap();
        assert_eq!(settings.log_folder, "mylogs");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_log_folder_defaults() {
        let dir = scratch_dir();
        fs::write(dir.join("grok.json5"), grok_json()).unwrap();
        let conf = write_valid_conf(&dir, "grok.json5", "");
        let settings = load(&conf).unwrap();
        assert_eq!(settings.log_folder, keys::DEFAULT_LOG_FOLDER);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_log_folder_is_error() {
        let dir = scratch_dir();
        fs::write(dir.join("grok.json5"), grok_json()).unwrap();
        let conf = write_valid_conf(&dir, "grok.json5", "\"log-folder\": \"\"");
        let err = load(&conf).unwrap_err().to_string();
        assert_eq!(err, "'log-folder' must be a non-empty string");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_key_is_error() {
        let err = load_raw(
            r#"{
                "cwd": "x",
                "paths": { "log-folder": "logs" },
                "agent": { "config": "g", "mode": "safe", "token-limit": 1, "iteration-limit": 1, "USD-limit": 1 }
            }"#,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            err,
            "'paths' was removed; move 'log-folder' into 'agent'"
        );
    }

    #[test]
    fn old_agent_key_is_rename_error() {
        let err = load_raw(
            r#"{
                "cwd": "x",
                "agent-setting": { "config": "g", "mode": "safe", "token-limit": 1, "iteration-limit": 1, "USD-limit": 1 }
            }"#,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "'agent-setting' was renamed to 'agent'");
    }

    #[test]
    fn both_agent_keys_is_rename_error() {
        let err = load_raw(
            r#"{
                "cwd": "x",
                "agent": { "config": "g", "mode": "safe", "token-limit": 1, "iteration-limit": 1, "USD-limit": 1 },
                "agent-setting": { "config": "g", "mode": "safe", "token-limit": 1, "iteration-limit": 1, "USD-limit": 1 }
            }"#,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "'agent-setting' was renamed to 'agent'");
    }

    #[test]
    fn template_after_vars_matches_new_schema() {
        let raw = template_text();
        assert!(raw.contains(&format!("\"{}\"", keys::AGENT)));
        assert!(raw.contains(&format!("\"{}\"", keys::LOG_FOLDER)));

        let substituted = crate::vars::substitute(&raw, "template").unwrap();
        let root: Value = json5::from_str(&substituted).unwrap();
        let obj = root.as_object().expect("template root");
        assert!(obj.contains_key(keys::VARS));
        assert!(obj.contains_key(keys::CWD));
        assert!(obj.contains_key(keys::AGENT));
        assert!(obj.contains_key(keys::TOOLS));
        assert!(!obj.contains_key("paths"));
        assert!(!obj.contains_key("agent-setting"));

        let agent = obj
            .get(keys::AGENT)
            .and_then(Value::as_object)
            .expect("agent object");
        assert!(agent.contains_key(keys::LOG_FOLDER));
        assert!(agent.contains_key(keys::CONFIG));
        assert!(agent.contains_key(keys::MODE));
    }

    #[test]
    fn config_file_loads() {
        let dir = scratch_dir();
        fs::write(dir.join("grok.json5"), grok_json()).unwrap();
        let conf = write_valid_conf(&dir, "grok.json5", "");
        let settings = load(&conf).unwrap();
        assert_eq!(settings.agent.name, "Grok 4.6");
        assert_eq!(settings.agent.config_path.file_name().unwrap(), "grok.json5");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_directory_loads() {
        let dir = scratch_dir();
        let agents = dir.join("agents");
        fs::create_dir(&agents).unwrap();
        fs::write(agents.join("grok.json5"), grok_json()).unwrap();
        let conf = write_valid_conf(&dir, "agents", "");
        let settings = load_with(&conf, |_| Ok(0)).unwrap();
        assert_eq!(settings.agent.name, "Grok 4.6");
        assert_eq!(settings.agent.config_path.file_name().unwrap(), "grok.json5");
        let _ = fs::remove_dir_all(&dir);
    }
}
