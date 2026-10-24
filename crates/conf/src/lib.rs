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
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::scan::scan_agent_dir;

    fn unique_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("harness-conf-{tag}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_model(dir: &std::path::Path, file: &str, body: &str) -> PathBuf {
        let path = dir.join(file);
        fs::write(&path, body).unwrap();
        path
    }

    fn model_body(vendor: &str, name: &str, rpm: i64, tpm: i64, rpd: i64) -> String {
        format!(
            r#"{{
                "vendor": "{vendor}",
                "name": "{name}",
                "api-key": "test-key",
                "token-limit": -1,
                "iteration-limit": 50,
                "USD-limit": -1,
                "rpm": {rpm},
                "tpm": {tpm},
                "rpd": {rpd}
            }}"#
        )
    }

    #[test]
    fn scan_finds_new_deepseek_file() {
        let dir = unique_dir("scan");
        write_model(
            &dir,
            "deepseek-v3.json5",
            r#"{ "vendor": "DeepSeek", "name": "DeepSeek-V3" }"#,
        );
        let (candidates, skipped) = scan_agent_dir(&dir).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].vendor, "DeepSeek");
        assert_eq!(candidates[0].name, "DeepSeek-V3");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_model_rpm_disabled_and_positive() {
        let dir = unique_dir("load");
        let minus_one = write_model(
            &dir,
            "deepseek-r1.json5",
            &model_body("DeepSeek", "DeepSeek-R1", -1, -1, -1),
        );
        let plus = write_model(
            &dir,
            "qwen-turbo.json5",
            &model_body("AliBaba", "Qwen-Turbo", 1500, 1_000_000, -1),
        );

        let a = load_model(&minus_one, Mode::AutoAccept).unwrap();
        assert_eq!(a.rpm, -1);
        assert_eq!(a.tpm, -1);
        assert_eq!(a.rpd, -1);
        assert!(matches!(a.kind, AgentKind::DeepSeek { .. }));

        let b = load_model(&plus, Mode::AutoAccept).unwrap();
        assert_eq!(b.rpm, 1500);
        assert_eq!(b.tpm, 1_000_000);
        assert_eq!(b.rpd, -1);
        assert!(matches!(b.kind, AgentKind::Alibaba { api_model } if api_model == "qwen-turbo"));

        let _ = fs::remove_dir_all(&dir);
    }
}
