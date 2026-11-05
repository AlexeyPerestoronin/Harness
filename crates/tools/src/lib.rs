//! Tool registry for the native harness.
//!
//! Each tool-set lives in its own module (`fs`, `git`, `terminal`, `user`,
//! `skills`, `roles`). The registry is assembled from the conf `tools` object:
//! a missing key skips the module, an unknown name is rejected at load time by
//! `conf`. Agents see a flat list of descriptors and call tools by name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use interactions::{Interaction, OutputType};
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

#[derive(Clone)]
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
            Module::Terminal(m) => m.descriptors(),
            Module::User(_) => UserModule::descriptors(),
            Module::Skills(m) => m.descriptors(),
            Module::Roles(m) => m.descriptors(),
        }
    }

    /// Synchronous tool modules only. `UserModule` is dispatched asynchronously.
    fn call_sync(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match self {
            Module::Fs(m) => m.call(name, args),
            Module::Git(m) => m.call(name, args),
            Module::Terminal(m) => m.call(name, args),
            Module::User(_) => Err(format!("tool '{name}' is asynchronous")),
            Module::Skills(m) => m.call(name, args),
            Module::Roles(m) => m.call(name, args),
        }
    }
}

/// Runtime context supplied by the agent loop when building the registry.
pub struct ToolsContext {
    /// Agent workspace (`settings.cwd`).
    pub cwd: PathBuf,
    /// Session directory used as `temp-dir` for patches.
    pub temp_dir: PathBuf,
    /// Shared user interaction contract.
    pub interaction: Arc<dyn Interaction>,
}

/// Flat dispatch table of enabled tool modules.
pub struct ToolRegistry {
    modules: Vec<Module>,
    interaction: Arc<dyn Interaction>,
}

impl ToolRegistry {
    /// Assemble modules from typed conf settings.
    pub fn build(settings: &conf::ToolsSettings, ctx: ToolsContext) -> Result<Self, String> {
        let _ = std::fs::create_dir_all(&ctx.temp_dir);

        let mut modules = Vec::new();
        let (fs_module, sandbox) = FsModule::build(settings.fs.as_ref(), &ctx.cwd)?;
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
            )));
        }
        if settings.user.is_some() {
            modules.push(Module::User(UserModule::new(
                sandbox,
                command_timeout,
                ctx.interaction.clone(),
            )));
        }
        if let Some(skills) = settings.skills.clone() {
            modules.push(Module::Skills(SkillsModule::new(skills)));
        }
        if let Some(roles) = settings.roles.clone() {
            modules.push(Module::Roles(RolesModule::new(roles)));
        }

        Ok(Self {
            modules,
            interaction: ctx.interaction,
        })
    }

    /// Descriptors of every enabled module, in registration order.
    pub fn list(&self) -> Vec<ToolDescriptor> {
        self.modules.iter().flat_map(|m| m.descriptors()).collect()
    }

    /// Dispatch `name` to the owning module and log the result like Python `DefaultTools.call`.
    ///
    /// Blocking modules (`fs`, `git`, `terminal`, `skills`, `roles`) run on the
    /// blocking thread pool; `user` is natively asynchronous because it talks
    /// to the [`Interaction`] contract.
    pub async fn call(&self, name: &str, args: Value) -> Result<String, String> {
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
        let result = if let Module::User(user) = module {
            user.call(name, &map).await?
        } else {
            let name_owned = name.to_string();
            let module = module.clone();
            tokio::task::spawn_blocking(move || module.call_sync(&name_owned, &map))
                .await
                .map_err(|e| format!("tool '{name}' execution failed: {e}"))??
        };
        self.log_result(&result).await;
        Ok(result.output)
    }

    async fn log_result(&self, result: &CallResult) {
        match &result.kind {
            CallKind::Patch { tmp } => {
                let _ = self
                    .interaction
                    .output(&format!("Apply patch {tmp}"), true, OutputType::NewLine)
                    .await;
            }
            CallKind::Read => {
                let _ = self
                    .interaction
                    .output(
                        &format!("Read {}symbols", result.output.len()),
                        true,
                        OutputType::NewLine,
                    )
                    .await;
            }
            CallKind::Write => {
                let _ = self
                    .interaction
                    .output(
                        &format!("Write {}symbols", result.output.len()),
                        true,
                        OutputType::NewLine,
                    )
                    .await;
            }
            CallKind::Normal => {
                let _ = self
                    .interaction
                    .output("Result:", true, OutputType::NewLine)
                    .await;
                let _ = self
                    .interaction
                    .output("```", true, OutputType::NewLine)
                    .await;
                let _ = self
                    .interaction
                    .output(&result.output, true, OutputType::NewLine)
                    .await;
                let _ = self
                    .interaction
                    .output("```", true, OutputType::NewLine)
                    .await;
            }
        }
    }
}

/// Required string argument. Numbers are stringified (models sometimes send depth as a number).
pub fn arg_str(args: &Map<String, Value>, key: &str) -> Result<String, String> {
    match args.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(Value::Bool(_)) => Err(format!("argument '{key}' must be a string, got boolean")),
        Some(_) => Err(format!("argument '{key}' must be a string")),
        None => Err(format!("missing argument '{key}'")),
    }
}

/// Optional string argument; missing or null becomes `""`.
pub fn arg_str_or_empty(args: &Map<String, Value>, key: &str) -> String {
    arg_str(args, key).unwrap_or_default()
}

/// Integer tool-call argument with a default.
///
/// Accepts a JSON number representable as `i64` (no fractional part) or a
/// string containing an integer. Missing and null values return `default`.
/// Booleans, objects, arrays, fractional numbers and values outside `i64`
/// are rejected.
pub fn arg_i64(args: &Map<String, Value>, key: &str, default: i64) -> Result<i64, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| format!("argument '{key}' must be an integer in i64 range")),
        Some(Value::String(s)) => s
            .parse::<i64>()
            .map_err(|_| format!("argument '{key}' must be a string containing an integer")),
        Some(Value::Bool(_)) => Err(format!("argument '{key}' must be an integer")),
        Some(other) => Err(format!("argument '{key}' must be an integer, got {other}")),
    }
}

/// Validate `output_buff_size`: `-1` (full output) or `>= 0` (last N chars).
pub fn validate_output_buff_size(value: i64) -> Result<i64, String> {
    if value == -1 || value >= 0 {
        Ok(value)
    } else {
        Err(format!(
            "argument 'output_buff_size' must be -1 or >= 0, got {value}"
        ))
    }
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
    use super::{arg_i64, arg_str, validate_output_buff_size};
    use serde_json::{json, Map, Value};

    fn args(value: Value) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("v".to_string(), value);
        map
    }

    #[test]
    fn arg_str_accepts_string_and_json_number() {
        assert_eq!(arg_str(&args(json!("path")), "v"), Ok("path".to_string()));
        assert_eq!(arg_str(&args(json!(42)), "v"), Ok("42".to_string()));
    }

    #[test]
    fn arg_str_rejects_bool() {
        assert!(arg_str(&args(json!(true)), "v").is_err());
        assert!(arg_str(&args(json!(false)), "v").is_err());
    }

    #[test]
    fn arg_str_rejects_object_array_null_and_reports_missing() {
        assert!(arg_str(&args(json!({})), "v").is_err());
        assert!(arg_str(&args(json!([])), "v").is_err());
        assert!(arg_str(&args(Value::Null), "v").is_err());
        let err = arg_str(&Map::new(), "v").unwrap_err();
        assert!(err.contains("missing argument 'v'"), "{err}");
    }

    #[test]
    fn arg_i64_accepts_json_integer_and_numeric_string() {
        assert_eq!(arg_i64(&args(json!(42)), "v", -1), Ok(42));
        assert_eq!(arg_i64(&args(json!(-7)), "v", -1), Ok(-7));
        assert_eq!(arg_i64(&args(json!("42")), "v", -1), Ok(42));
        assert_eq!(arg_i64(&args(json!("-7")), "v", -1), Ok(-7));
    }

    #[test]
    fn arg_i64_uses_default_for_missing_and_null() {
        assert_eq!(arg_i64(&Map::new(), "v", -1), Ok(-1));
        assert_eq!(arg_i64(&args(Value::Null), "v", 5), Ok(5));
    }

    #[test]
    fn arg_i64_rejects_bool_object_array_fractional_and_out_of_i64() {
        assert!(arg_i64(&args(json!(true)), "v", -1).is_err());
        assert!(arg_i64(&args(json!({})), "v", -1).is_err());
        assert!(arg_i64(&args(json!([])), "v", -1).is_err());
        assert!(arg_i64(&args(json!(1.5)), "v", -1).is_err());
        assert!(arg_i64(&args(json!("1.5")), "v", -1).is_err());
        assert!(arg_i64(&args(json!("999999999999999999999999")), "v", -1).is_err());
        assert!(arg_i64(&args(serde_json::from_str::<Value>("9223372036854775808").unwrap()), "v", -1).is_err());
    }

    #[test]
    fn validate_output_buff_size_accepts_minus_one_and_non_negative() {
        assert_eq!(validate_output_buff_size(-1), Ok(-1));
        assert_eq!(validate_output_buff_size(0), Ok(0));
        assert_eq!(validate_output_buff_size(10), Ok(10));
    }

    #[test]
    fn validate_output_buff_size_rejects_values_below_minus_one() {
        assert!(validate_output_buff_size(-2).is_err());
    }
}
