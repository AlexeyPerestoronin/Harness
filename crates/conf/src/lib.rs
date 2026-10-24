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
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{json, Value};

    use super::str::keys;
    use super::{load, load_from_value, template_text, Settings};

    static SEQ: AtomicU64 = AtomicU64::new(1);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "harness-conf-{}-{}-{}-{}",
                label,
                std::process::id(),
                n,
                nanos
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dummy_agent() -> &'static str {
        r#"{
            "vendor": "SpaceXAI",
            "name": "Grok 4.6",
            "api-key": "test-key"
        }"#
    }

    fn write_agent(dir: &Path) {
        fs::write(dir.join("agent.json5"), dummy_agent()).unwrap();
    }

    fn root_with_tools(dir: &Path, tools: Value) -> Value {
        json!({
            "cwd": dir.to_string_lossy(),
            "agent": {
                "config": "agent.json5",
                "mode": "safe",
                "token-limit": -1,
                "iteration-limit": -1,
                "USD-limit": -1
            },
            "tools": tools
        })
    }

    fn load_tools(dir: &Path, tools: Value) -> Result<Settings, String> {
        load_from_value(root_with_tools(dir, tools), dir, |_| {
            Err("agent was not selected".into())
        })
        .map_err(|e| e.to_string())
    }

    #[test]
    fn template_contains_skill_and_role_keys() {
        let text = template_text();
        assert!(text.contains(keys::TOOL_SKILLS));
        assert!(text.contains(keys::TOOL_ROLES));
        assert!(text.contains(keys::DESCRIPTION));
        assert!(text.contains(keys::FILE));
    }

    #[test]
    fn valid_skills_and_roles_resolve_absolute_paths() {
        let tmp = TempDir::new("valid");
        let dir = tmp.path();
        write_agent(dir);
        fs::write(dir.join("rust.md"), "rust skill body").unwrap();
        fs::write(dir.join("senior.md"), "senior role body").unwrap();

        let settings = load_tools(
            dir,
            json!({
                "skills": {
                    "rust-programmer": {
                        "description": "Rust",
                        "file": "rust.md"
                    }
                },
                "roles": {
                    "senior-developer": {
                        "description": "Senior",
                        "file": "senior.md"
                    }
                }
            }),
        )
        .unwrap();

        let skills = settings.tools.skills.expect("skills enabled");
        let rust = skills.items.get("rust-programmer").expect("rust skill");
        assert_eq!(rust.description, "Rust");
        assert_eq!(rust.file, dir.join("rust.md"));
        assert!(rust.file.is_absolute());

        let roles = settings.tools.roles.expect("roles enabled");
        let senior = roles.items.get("senior-developer").expect("senior role");
        assert_eq!(senior.description, "Senior");
        assert_eq!(senior.file, dir.join("senior.md"));
        assert!(senior.file.is_absolute());
    }

    #[test]
    fn relative_file_is_resolved_from_conf_dir() {
        let tmp = TempDir::new("relative");
        let dir = tmp.path();
        write_agent(dir);
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested").join("skill.md"), "body").unwrap();

        let settings = load_tools(
            dir,
            json!({
                "skills": {
                    "nested.skill": {
                        "description": "Nested",
                        "file": "nested/skill.md"
                    }
                }
            }),
        )
        .unwrap();

        let file = &settings.tools.skills.unwrap().items["nested.skill"].file;
        assert_eq!(file, &dir.join("nested").join("skill.md"));
        assert!(file.is_absolute());
    }

    #[test]
    fn missing_skill_file_uses_exact_message() {
        let tmp = TempDir::new("missing-skill");
        let dir = tmp.path();
        write_agent(dir);
        let missing = dir.join("nope.md");
        let err = load_tools(
            dir,
            json!({
                "skills": {
                    "gone": {
                        "description": "Gone",
                        "file": "nope.md"
                    }
                }
            }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            format!("error → skill-file '{}' doesn't find", missing.display())
        );
    }

    #[test]
    fn missing_role_file_uses_exact_message() {
        let tmp = TempDir::new("missing-role");
        let dir = tmp.path();
        write_agent(dir);
        let missing = dir.join("nope.md");
        let err = load_tools(
            dir,
            json!({
                "roles": {
                    "gone": {
                        "description": "Gone",
                        "file": "nope.md"
                    }
                }
            }),
        )
        .unwrap_err();
        assert_eq!(
            err,
            format!("error → role-file '{}' doesn't find", missing.display())
        );
    }

    #[test]
    fn missing_skills_key_disables_module() {
        let tmp = TempDir::new("no-skills");
        let dir = tmp.path();
        write_agent(dir);
        let settings = load_tools(dir, json!({})).unwrap();
        assert!(settings.tools.skills.is_none());
        assert!(settings.tools.roles.is_none());
    }

    #[test]
    fn empty_skills_object_enables_module() {
        let tmp = TempDir::new("empty-skills");
        let dir = tmp.path();
        write_agent(dir);
        let settings = load_tools(dir, json!({ "skills": {}, "roles": {} })).unwrap();
        let skills = settings.tools.skills.expect("skills enabled");
        assert!(skills.items.is_empty());
        let roles = settings.tools.roles.expect("roles enabled");
        assert!(roles.items.is_empty());
    }

    #[test]
    fn template_loads_with_dummy_agent() {
        let tmp = TempDir::new("template");
        let dir = tmp.path();
        fs::write(dir.join("grok-4.6.json5"), dummy_agent()).unwrap();
        let conf_path = dir.join("harness.json5");
        fs::write(&conf_path, template_text()).unwrap();
        let settings = load(&conf_path).unwrap();
        let skills = settings.tools.skills.expect("skills in template");
        assert!(skills.items.is_empty());
        let roles = settings.tools.roles.expect("roles in template");
        assert!(roles.items.is_empty());
    }

    #[test]
    fn unknown_key_in_skill_entry_is_error() {
        let tmp = TempDir::new("unknown-key");
        let dir = tmp.path();
        write_agent(dir);
        fs::write(dir.join("a.md"), "a").unwrap();
        let err = load_tools(
            dir,
            json!({
                "skills": {
                    "a": {
                        "description": "A",
                        "file": "a.md",
                        "extra": "nope"
                    }
                }
            }),
        )
        .unwrap_err();
        assert_eq!(err, "unknown key 'extra' in skill 'a'");
    }
}
