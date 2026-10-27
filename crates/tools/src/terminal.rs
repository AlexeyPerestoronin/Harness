//! Terminal tool set (`tools.terminal`): whitelisted `run_shell`.
//!
//! `run_shell` runs a command from `available-commands` in a directory that
//! must be inside both the root CWD (`settings.cwd`) and one of the
//! `cwd-list` entries. `cwd-list` entries are resolved against the root CWD,
//! not the process cwd.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::fs::abspath;
use crate::process::run_observed_command;
use crate::{arg_str, arg_str_or_empty, CallResult, ToolDescriptor};

/// Shell tools bound to the terminal allow-lists.
pub struct TerminalModule {
    /// Root workspace directory from `settings.cwd`.
    cwd: PathBuf,
    /// Directories from which `run_shell` may execute.
    cwd_list: Vec<String>,
    /// Command names allowed for `run_shell`.
    available_commands: Vec<String>,
    /// Timeout for each `run_shell` subprocess, seconds.
    command_execution_limit: u64,
}

impl TerminalModule {
    /// Construct from typed `tools.terminal` values.
    pub fn new(
        cwd: PathBuf,
        cwd_list: Vec<String>,
        available_commands: Vec<String>,
        command_execution_limit: u64,
    ) -> Self {
        Self {
            cwd,
            cwd_list,
            available_commands,
            command_execution_limit,
        }
    }

    /// JSON Schema descriptor for `run_shell`.
    ///
    /// The description is built from the actual module settings so the model
    /// always sees the concrete `available-commands` and `cwd-list` values.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        vec![desc(
            "run_shell",
            &format!(
                "Run a command. available-commands: {:?}. cwd-list: {:?}. cwd must be inside the root CWD '{}' and inside one of the cwd-list entries (subdirectories are allowed).",
                self.available_commands,
                self.cwd_list,
                path_string(&self.root_cwd())
            ),
            json!({"type":"object","properties":{"cwd":{"type":"string"},"command":{"type":"string"},"arguments":{"type":"string"}},"required":["cwd","command"]}),
        )]
    }

    /// Dispatch a named terminal tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "run_shell" => Ok(CallResult::plain(self.run_shell(
                &arg_str(args, "cwd")?,
                &arg_str(args, "command")?,
                &arg_str_or_empty(args, "arguments"),
            )?)),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn run_shell(&self, cwd: &str, command: &str, arguments: &str) -> Result<String, String> {
        if !self.available_commands.iter().any(|c| c == command) {
            return Err(format!(
                "'{command}'-command is not available (available command list is {:?})",
                self.available_commands
            ));
        }
        let run_cwd = self.validated_cwd(cwd, command)?;
        let cmd = format!("{command} {arguments}");
        run_observed_command(&cmd, &run_cwd, self.command_execution_limit)
    }

    /// Validate `cwd` and return its normalized absolute path.
    ///
    /// The path is resolved against the root CWD when relative, normalized
    /// (`.`, `..`), and then checked to stay inside both the root CWD and at
    /// least one `cwd-list` entry.
    fn validated_cwd(&self, cwd: &str, command: &str) -> Result<PathBuf, String> {
        let root = self.root_cwd();
        let requested = resolve_from(&root, cwd);
        let denied = |reason: &str| {
            format!(
                "'{cwd}'-cwd is denied for '{command}'-command: {reason} (requested cwd: '{cwd}', available cwd-list: {:?}, root cwd: '{}')",
                self.cwd_list,
                path_string(&root)
            )
        };
        if !requested.starts_with(&root) {
            return Err(denied("it is outside the root cwd"));
        }
        let in_list = self
            .cwd_list
            .iter()
            .any(|entry| requested.starts_with(&resolve_from(&root, entry)));
        if !in_list {
            return Err(denied("it is outside every cwd-list entry"));
        }
        Ok(requested)
    }

    fn root_cwd(&self) -> PathBuf {
        abspath(&path_string(&self.cwd))
    }
}

/// Resolve `path` as an absolute normalized path: relative paths are joined
/// with `root`, absolute paths are kept, then `.` / `..` are normalized.
fn resolve_from(root: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        abspath(path)
    } else {
        abspath(&path_string(&root.join(candidate)))
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}
