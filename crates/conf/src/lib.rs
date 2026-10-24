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
pub use load::{load, load_from_value, load_model, load_with};
pub use template::{template_text, write_new_conf};
pub use types::{
    AgentCandidate, AgentKind, AgentSettings, FsSettings, GitSettings, Mode, RoleEntry,
    RolesSettings, Settings, SkillEntry, SkillsSettings, TerminalSettings, ToolsSettings,
    UserSettings,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn agents_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../agents")
    }

    fn write_temp_model(body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "harness-rpm-test-{}-{}.json5",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, body).unwrap();
        path
    }

    fn grok_body(extra: &str) -> String {
        format!(
            r#"{{
                "vendor": "SpaceXAI",
                "name": "Grok 4.6",
                "api-key": "test-key",
                "token-limit": 1,
                "iteration-limit": 1,
                "USD-limit": 1
                {extra}
            }}"#
        )
    }

    #[test]
    fn grok_file_with_disabled_limits_loads() {
        let path = agents_dir().join("grok-4.6.json5");
        let settings = load_model(&path, Mode::Safe).expect("grok model");
        assert_eq!(settings.rpm, -1);
        assert_eq!(settings.tpm, -1);
        assert_eq!(settings.rpd, -1);
    }

    #[test]
    fn gemini_file_keeps_positive_limits() {
        let path = agents_dir().join("gemini-3.8-flash.json5");
        let settings = load_model(&path, Mode::Safe).expect("gemini model");
        assert_eq!(settings.rpm, 5);
        assert_eq!(settings.tpm, 250000);
        assert_eq!(settings.rpd, 20);
    }

    #[test]
    fn missing_rpm_is_error() {
        let path = write_temp_model(&grok_body(r#", "tpm": -1, "rpd": -1"#));
        let err = load_model(&path, Mode::Safe).expect_err("rpm required");
        let _ = fs::remove_file(&path);
        assert_eq!(
            err.to_string(),
            "please define 'rpm' in the model conf file"
        );
    }

    #[test]
    fn rpm_zero_is_error() {
        let path = write_temp_model(&grok_body(r#", "rpm": 0, "tpm": -1, "rpd": -1"#));
        let err = load_model(&path, Mode::Safe).expect_err("rpm 0 invalid");
        let _ = fs::remove_file(&path);
        assert_eq!(err.to_string(), "'rpm' must be -1 or a positive integer");
    }

    #[test]
    fn missing_tpm_is_error() {
        let path = write_temp_model(&grok_body(r#", "rpm": -1, "rpd": -1"#));
        let err = load_model(&path, Mode::Safe).expect_err("tpm required");
        let _ = fs::remove_file(&path);
        assert_eq!(
            err.to_string(),
            "please define 'tpm' in the model conf file"
        );
    }
}
