//! Tool registry for the native harness.
//!
//! Each tool-set lives in its own module (`fs`, `git`, `terminal`, `user`,
//! `skills`, `roles`). The registry is assembled from the conf `tools` object:
//! a missing key skips the module, an unknown name is rejected at load time by
//! `conf`. Agents see a flat list of descriptors and call tools by name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use logger::Logger;
use serde_json::{Map, Value};

pub mod fs;
pub mod git;
pub mod process;
pub mod roles;
pub mod skills;
pub mod terminal;
pub mod user;

use fs::FsModule;
use git::GitModule;
use roles::RolesModule;
use skills::SkillsModule;
use terminal::TerminalModule;
use user::UserModule;

/// Human-readable description of every tool module and its conf parameters.
///
/// Printed by `harness.exe --help-tools`. Embedded at compile time from
/// `crates/tools/help-user.txt`.
pub const HELP_USER: &str = include_str!("../help-user.txt");

/// JSON Schema descriptor exposed to the agent as one tool.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    /// Tool name (function name in the vendor API).
    pub name: String,
    /// Short description sent to the model (not `tool_help.md`).
    pub description: String,
    /// JSON Schema `parameters` object.
    pub parameters: Value,
}

/// How [`ToolRegistry::call`] should log the result.
#[derive(Debug, Clone)]
pub enum CallKind {
    /// Default: `Result:` + markdown fence.
    Normal,
    /// `read_file`: log `Read Nsymbols`.
    Read,
    /// `write_file`: log `Write Nsymbols`.
    Write,
    /// `apply_diff_patch`: log `Apply patch {tmp}`.
    Patch {
        /// Temporary patch file written for `git apply`.
        tmp: String,
    },
}

/// Result of a single tool invocation.
#[derive(Debug, Clone)]
pub struct CallResult {
    /// Text returned to the agent.
    pub output: String,
    /// Logging mode for the registry.
    pub kind: CallKind,
}

impl CallResult {
    /// Plain result logged as a fenced `Result:` block.
    pub fn plain(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            kind: CallKind::Normal,
        }
    }
}

enum Module {
    Fs(FsModule),
    Git(GitModule),
    Terminal(TerminalModule),
    User(UserModule),
    Skills(SkillsModule),
    Roles(RolesModule),
}

impl Module {
    fn owns(&self, name: &str) -> bool {
        self.descriptors().iter().any(|d| d.name == name)
    }

    fn descriptors(&self) -> Vec<ToolDescriptor> {
        match self {
            Module::Fs(_) => FsModule::descriptors(),
            Module::Git(_) => GitModule::descriptors(),
            Module::Terminal(_) => TerminalModule::descriptors(),
            Module::User(_) => UserModule::descriptors(),
            Module::Skills(m) => m.descriptors(),
            Module::Roles(m) => m.descriptors(),
        }
    }

    fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match self {
            Module::Fs(m) => m.call(name, args),
            Module::Git(m) => m.call(name, args),
            Module::Terminal(m) => m.call(name, args),
            Module::User(m) => m.call(name, args),
            Module::Skills(m) => m.call(name, args),
            Module::Roles(m) => m.call(name, args),
        }
    }
}

/// Runtime context supplied by the agent loop when building the registry.
pub struct ToolsContext {
    /// Agent workspace (`settings.cwd`).
    pub cwd: PathBuf,
    /// Session log directory used as `temp-dir` for patches.
    pub temp_dir: PathBuf,
    /// Shared session logger.
    pub logger: Arc<dyn Logger>,
}

/// Flat dispatch table of enabled tool modules.
pub struct ToolRegistry {
    modules: Vec<Module>,
    logger: Arc<dyn Logger>,
}

impl ToolRegistry {
    /// Assemble modules from typed conf settings.
    pub fn build(settings: &conf::ToolsSettings, ctx: ToolsContext) -> Self {
        let _ = std::fs::create_dir_all(&ctx.temp_dir);

        let mut modules = Vec::new();
        let (fs_module, sandbox) = FsModule::build(settings.fs.as_ref());
        if let Some(m) = fs_module {
            modules.push(Module::Fs(m));
        }
        if let Some(git) = &settings.git {
            modules.push(Module::Git(GitModule::new(
                sandbox.clone(),
                git.executable.clone(),
                git.execution_limit,
                ctx.cwd.clone(),
                ctx.temp_dir.clone(),
            )));
        }
        let command_timeout = settings
            .terminal
            .as_ref()
            .map(|t| t.command_execution_limit);
        if let Some(term) = &settings.terminal {
            modules.push(Module::Terminal(TerminalModule::new(
                ctx.cwd.clone(),
                term.cwd_list.clone(),
                term.available_commands.clone(),
                term.command_execution_limit,
                term.invoke_execution_limit,
            )));
        }
        if settings.user.is_some() {
            modules.push(Module::User(UserModule::new(sandbox, command_timeout)));
        }
        if let Some(skills) = settings.skills.clone() {
            modules.push(Module::Skills(SkillsModule::new(skills)));
        }
        if let Some(roles) = settings.roles.clone() {
            modules.push(Module::Roles(RolesModule::new(roles)));
        }

        Self {
            modules,
            logger: ctx.logger,
        }
    }

    /// Descriptors of every enabled module, in registration order.
    pub fn list(&self) -> Vec<ToolDescriptor> {
        self.modules.iter().flat_map(|m| m.descriptors()).collect()
    }

    /// Dispatch `name` to the owning module and log the result like Python `DefaultTools.call`.
    pub fn call(&self, name: &str, args: Value) -> Result<String, String> {
        let map = match args {
            Value::Object(m) => m,
            Value::Null => Map::new(),
            other => {
                return Err(format!(
                    "tool '{name}' arguments must be an object, got {other}"
                ))
            }
        };
        let module = self
            .modules
            .iter()
            .find(|m| m.owns(name))
            .ok_or_else(|| format!("tool '{name}' not available"))?;
        let result = module.call(name, &map)?;
        match &result.kind {
            CallKind::Patch { tmp } => self.logger.log_line(&format!("Apply patch {tmp}")),
            CallKind::Read => self
                .logger
                .log_line(&format!("Read {}symbols", result.output.len())),
            CallKind::Write => self
                .logger
                .log_line(&format!("Write {}symbols", result.output.len())),
            CallKind::Normal => {
                self.logger.log_line("Result:");
                self.logger.log_line("```");
                self.logger.log_line(&result.output);
                self.logger.log_line("```");
            }
        }
        Ok(result.output)
    }
}

/// Required string argument. Numbers are stringified (models sometimes send depth as a number).
pub fn arg_str(args: &Map<String, Value>, key: &str) -> Result<String, String> {
    match args.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(Value::Bool(b)) => Ok(b.to_string()),
        Some(_) => Err(format!("argument '{key}' must be a string")),
        None => Err(format!("missing argument '{key}'")),
    }
}

/// Optional string argument; missing or null becomes `""`.
pub fn arg_str_or_empty(args: &Map<String, Value>, key: &str) -> String {
    arg_str(args, key).unwrap_or_default()
}

/// Read a text file whose path was set by the conf owner (skills / roles).
///
/// Not sandboxed: the agent does not choose the path.
fn read_conf_owned_file(path: &Path, action: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("{action} → cannot read '{}': {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use conf::{RoleEntry, RolesSettings, SkillEntry, SkillsSettings, ToolsSettings};
    use logger::NullLogger;
    use serde_json::json;

    use super::{ToolRegistry, ToolsContext};

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
                "harness-tools-{}-{}-{}-{}",
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

    fn registry(settings: &ToolsSettings, dir: &Path) -> ToolRegistry {
        ToolRegistry::build(
            settings,
            ToolsContext {
                cwd: dir.to_path_buf(),
                temp_dir: dir.join("tmp"),
                logger: Arc::new(NullLogger),
            },
        )
    }

    fn names(reg: &ToolRegistry) -> Vec<String> {
        reg.list().into_iter().map(|d| d.name).collect()
    }

    #[test]
    fn skills_some_registers_get_skill_explanation() {
        let tmp = TempDir::new("skills-some");
        let mut settings = ToolsSettings::default();
        settings.skills = Some(SkillsSettings::default());
        let reg = registry(&settings, tmp.path());
        assert!(names(&reg).contains(&"get_skill_explanation".to_string()));
        assert!(!names(&reg).contains(&"get_role_explanation".to_string()));
    }

    #[test]
    fn skills_none_does_not_register_tool() {
        let tmp = TempDir::new("skills-none");
        let reg = registry(&ToolsSettings::default(), tmp.path());
        assert!(!names(&reg).contains(&"get_skill_explanation".to_string()));
        assert!(!names(&reg).contains(&"get_role_explanation".to_string()));
    }

    #[test]
    fn roles_some_registers_get_role_explanation() {
        let tmp = TempDir::new("roles-some");
        let mut settings = ToolsSettings::default();
        settings.roles = Some(RolesSettings::default());
        let reg = registry(&settings, tmp.path());
        assert!(names(&reg).contains(&"get_role_explanation".to_string()));
    }

    #[test]
    fn get_skill_explanation_returns_file_contents() {
        let tmp = TempDir::new("skill-call");
        let file = tmp.path().join("rust.md");
        fs::write(&file, "full rust skill").unwrap();
        let mut settings = ToolsSettings::default();
        settings.skills = Some(SkillsSettings {
            items: [(
                "rust-programmer".to_string(),
                SkillEntry {
                    description: "Rust".to_string(),
                    file,
                },
            )]
            .into(),
        });
        let reg = registry(&settings, tmp.path());
        let out = reg
            .call(
                "get_skill_explanation",
                json!({"skill_name": "rust-programmer"}),
            )
            .unwrap();
        assert_eq!(out, "full rust skill");
    }

    #[test]
    fn unknown_skill_lists_available_names() {
        let tmp = TempDir::new("skill-unknown");
        let file_a = tmp.path().join("a.md");
        let file_b = tmp.path().join("b.md");
        fs::write(&file_a, "a").unwrap();
        fs::write(&file_b, "b").unwrap();
        let mut settings = ToolsSettings::default();
        settings.skills = Some(SkillsSettings {
            items: [
                (
                    "beta".to_string(),
                    SkillEntry {
                        description: "B".to_string(),
                        file: file_b,
                    },
                ),
                (
                    "alpha".to_string(),
                    SkillEntry {
                        description: "A".to_string(),
                        file: file_a,
                    },
                ),
            ]
            .into(),
        });
        let reg = registry(&settings, tmp.path());
        let err = reg
            .call("get_skill_explanation", json!({"skill_name": "nope"}))
            .unwrap_err();
        assert_eq!(
            err,
            "cannot get skill explanation → unknown skill 'nope'; available: alpha, beta"
        );
    }

    #[test]
    fn skill_descriptor_contains_configured_names() {
        let tmp = TempDir::new("skill-desc");
        let file = tmp.path().join("rust.md");
        fs::write(&file, "body").unwrap();
        let mut settings = ToolsSettings::default();
        settings.skills = Some(SkillsSettings {
            items: [(
                "rust-programmer".to_string(),
                SkillEntry {
                    description: "Used for Rust".to_string(),
                    file,
                },
            )]
            .into(),
        });
        let reg = registry(&settings, tmp.path());
        let desc = reg
            .list()
            .into_iter()
            .find(|d| d.name == "get_skill_explanation")
            .unwrap();
        assert!(desc.description.contains("rust-programmer"));
        assert!(desc.description.contains("Used for Rust"));
        assert!(desc.description.contains("—"));
    }

    #[test]
    fn get_role_explanation_returns_file_contents() {
        let tmp = TempDir::new("role-call");
        let file = tmp.path().join("senior.md");
        fs::write(&file, "full senior role").unwrap();
        let mut settings = ToolsSettings::default();
        settings.roles = Some(RolesSettings {
            items: [(
                "senior-developer".to_string(),
                RoleEntry {
                    description: "Senior".to_string(),
                    file,
                },
            )]
            .into(),
        });
        let reg = registry(&settings, tmp.path());
        let out = reg
            .call(
                "get_role_explanation",
                json!({"role_name": "senior-developer"}),
            )
            .unwrap();
        assert_eq!(out, "full senior role");
    }

    #[test]
    fn unknown_role_lists_available_names() {
        let tmp = TempDir::new("role-unknown");
        let file = tmp.path().join("senior.md");
        fs::write(&file, "body").unwrap();
        let mut settings = ToolsSettings::default();
        settings.roles = Some(RolesSettings {
            items: [(
                "senior-developer".to_string(),
                RoleEntry {
                    description: "Senior".to_string(),
                    file,
                },
            )]
            .into(),
        });
        let reg = registry(&settings, tmp.path());
        let err = reg
            .call("get_role_explanation", json!({"role_name": "nope"}))
            .unwrap_err();
        assert_eq!(
            err,
            "cannot get role explanation → unknown role 'nope'; available: senior-developer"
        );
    }

    #[test]
    fn empty_skills_descriptor_says_none_configured() {
        let tmp = TempDir::new("skill-empty-desc");
        let mut settings = ToolsSettings::default();
        settings.skills = Some(SkillsSettings::default());
        let reg = registry(&settings, tmp.path());
        let desc = reg
            .list()
            .into_iter()
            .find(|d| d.name == "get_skill_explanation")
            .unwrap();
        assert!(desc.description.contains("none configured"));
    }
}
