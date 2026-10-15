//! Tool registry for the native harness.
//!
//! Each tool-set lives in its own module (`fs`, `git`, `terminal`, `user`).
//! The registry is assembled from the conf `tools` object: a missing key skips
//! the module, an unknown name is rejected at load time by `conf`. Agents see
//! a flat list of descriptors and call tools by name.

use std::path::PathBuf;
use std::sync::Arc;

use logger::Logger;
use serde_json::{Map, Value};

pub mod fs;
pub mod git;
pub mod process;
pub mod terminal;
pub mod user;

use fs::FsModule;
use git::GitModule;
use terminal::TerminalModule;
use user::UserModule;

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
        }
    }

    fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match self {
            Module::Fs(m) => m.call(name, args),
            Module::Git(m) => m.call(name, args),
            Module::Terminal(m) => m.call(name, args),
            Module::User(m) => m.call(name, args),
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

        Self {
            modules,
            logger: ctx.logger,
        }
    }

    /// Descriptors of every enabled module, in registration order.
    pub fn list(&self) -> Vec<ToolDescriptor> {
        self.modules
            .iter()
            .flat_map(|m| m.descriptors())
            .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use conf::{FsSettings, GitSettings, TerminalSettings, ToolsSettings};
    use logger::NullLogger;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn ctx() -> ToolsContext {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("tools-reg-{nanos}"));
        ToolsContext {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            temp_dir: temp,
            logger: Arc::new(NullLogger),
        }
    }

    #[test]
    fn registry_builds_only_enabled_modules() {
        let mut settings = ToolsSettings::default();
        settings.fs = Some(FsSettings::default());
        let reg = ToolRegistry::build(&settings, ctx());
        let names: Vec<_> = reg.list().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"find_str".to_string()));
        assert!(names.contains(&"find_file_by_str".to_string()));
        assert!(names.contains(&"list_available_r_dir".to_string()));
        assert!(!names.contains(&"run_shell".to_string()));
        assert!(!names.contains(&"list_available_shell_commands".to_string()));
        assert!(!names.contains(&"ask_user".to_string()));
        assert!(!names.contains(&"is_file_under_git".to_string()));
    }

    #[test]
    fn registry_terminal_owns_shell_list() {
        let mut settings = ToolsSettings::default();
        settings.terminal = Some(TerminalSettings {
            cwd_list: vec![".".into()],
            available_commands: vec!["cargo".into()],
            command_execution_limit: 10,
            invoke_execution_limit: 10,
        });
        let reg = ToolRegistry::build(&settings, ctx());
        let names: Vec<_> = reg.list().into_iter().map(|d| d.name).collect();
        assert!(names.contains(&"run_shell".to_string()));
        assert!(names.contains(&"list_available_shell_commands".to_string()));
        assert!(!names.contains(&"list_available_r_dir".to_string()));
        assert!(!names.contains(&"find_str".to_string()));
    }

    #[test]
    fn unknown_tool_name() {
        let mut settings = ToolsSettings::default();
        settings.fs = Some(FsSettings::default());
        let reg = ToolRegistry::build(&settings, ctx());
        let err = reg.call("nope", serde_json::json!({})).unwrap_err();
        assert_eq!(err, "tool 'nope' not available");
    }

    #[test]
    fn git_module_uses_conf_executable() {
        let mut settings = ToolsSettings::default();
        settings.git = Some(GitSettings {
            executable: "C:/custom/git.exe".into(),
            execution_limit: 10,
        });
        let _reg = ToolRegistry::build(&settings, ctx());
        let cmd = git::git_command("C:/custom/git.exe", &["status"]);
        assert_eq!(
            cmd.get_program(),
            std::ffi::OsStr::new("C:/custom/git.exe")
        );
    }

    #[test]
    fn dispatch_read_file_access_denied() {
        let mut settings = ToolsSettings::default();
        settings.fs = Some(FsSettings::default());
        let reg = ToolRegistry::build(&settings, ctx());
        let err = reg
            .call("read_file", serde_json::json!({"path": "nope.txt"}))
            .unwrap_err();
        assert!(err.contains("cannot read file"), "{err}");
    }
}
