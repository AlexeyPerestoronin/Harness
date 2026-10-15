//! Terminal tool set (`tools.terminal`): `run_shell`, `run_invoke`, and shell allow-list.
//!
//! `run_shell` requires the command in `available-commands` and cwd in `cwd-list`.
//! `run_invoke` does not use the whitelist; it launches `{cwd}/.venv/.../python -m invoke`.
//! `list_available_shell_commands` reports `available-commands`.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::fs::abspath;
use crate::process::run_observed_command;
use crate::{arg_str, arg_str_or_empty, CallResult, ToolDescriptor};

/// Shell / invoke tools.
pub struct TerminalModule {
    cwd: PathBuf,
    cwd_list: Vec<String>,
    available_commands: Vec<String>,
    command_execution_limit: u64,
    invoke_execution_limit: u64,
}

impl TerminalModule {
    /// Construct from conf values.
    pub fn new(
        cwd: PathBuf,
        cwd_list: Vec<String>,
        available_commands: Vec<String>,
        command_execution_limit: u64,
        invoke_execution_limit: u64,
    ) -> Self {
        Self {
            cwd,
            cwd_list,
            available_commands,
            command_execution_limit,
            invoke_execution_limit,
        }
    }

    /// Command names allowed for `run_shell`.
    pub fn available_commands(&self) -> &[String] {
        &self.available_commands
    }

    /// JSON Schema descriptors for the terminal tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("run_shell", "Run a command.", json!({"type":"object","properties":{"cwd":{"type":"string"},"command":{"type":"string"},"arguments":{"type":"string"}},"required":["cwd","command"]})),
            desc("run_invoke", "Run an invoke command.", json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]})),
            desc("list_available_shell_commands", "List of available shell commands.", json!({})),
        ]
    }

    /// Dispatch a named terminal tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "run_shell" => Ok(CallResult::plain(self.run_shell(
                &arg_str(args, "cwd")?,
                &arg_str(args, "command")?,
                &arg_str_or_empty(args, "arguments"),
            )?)),
            "run_invoke" => Ok(CallResult::plain(self.run_invoke(&arg_str(args, "command")?)?)),
            "list_available_shell_commands" => Ok(CallResult::plain(format!(
                "available: {:?}",
                self.available_commands()
            ))),
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
        if !cwd_allowed(cwd, &self.cwd_list) {
            return Err(format!(
                "'{cwd}'-cwd is denied for '{command}'-command (allowed cwd for '{command}'-command is {:?})",
                self.cwd_list
            ));
        }
        let cmd = format!("{command} {arguments}");
        run_observed_command(&cmd, Path::new(cwd), self.command_execution_limit)
    }

    fn run_invoke(&self, command: &str) -> Result<String, String> {
        let python = invoke_python(&self.cwd);
        let cmd = format!("{} -m invoke {command}", python.display());
        run_observed_command(&cmd, &self.cwd, self.invoke_execution_limit)
    }
}

fn cwd_allowed(cwd: &str, list: &[String]) -> bool {
    let abs = path_string(&abspath(cwd));
    list.iter()
        .any(|allowed| abs.starts_with(&path_string(&abspath(allowed))))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn invoke_python(cwd: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        cwd.join(".venv").join("Scripts").join("python.exe")
    }
    #[cfg(not(windows))]
    {
        cwd.join(".venv").join("bin").join("python")
    }
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("term-mod-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
            .collect()
    }

    #[test]
    fn unknown_command_and_cwd_denied() {
        let root = tmp();
        let m = TerminalModule::new(
            root.clone(),
            vec![root.to_string_lossy().into_owned()],
            vec!["echo".into()],
            5,
            5,
        );
        let err = m
            .call(
                "run_shell",
                &args(&[
                    ("cwd", &root.to_string_lossy()),
                    ("command", "rm"),
                    ("arguments", "-rf /"),
                ]),
            )
            .unwrap_err();
        assert!(err.contains("not available"), "{err}");

        let outside = std::env::temp_dir().join("term-outside-cwd");
        let err = m
            .call(
                "run_shell",
                &args(&[
                    ("cwd", &outside.to_string_lossy()),
                    ("command", "echo"),
                ]),
            )
            .unwrap_err();
        assert!(err.contains("cwd is denied"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_arguments_is_empty_string() {
        let root = tmp();
        let m = TerminalModule::new(
            root.clone(),
            vec![root.to_string_lossy().into_owned()],
            vec!["echo".into()],
            5,
            5,
        );
        // `arguments` omitted — treated as "".
        let mut map = Map::new();
        map.insert("cwd".into(), Value::String(root.to_string_lossy().into_owned()));
        map.insert("command".into(), Value::String("echo".into()));
        let result = m.call("run_shell", &map);
        assert!(result.is_ok(), "{result:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invoke_python_path_layout() {
        let cwd = PathBuf::from("/ws");
        let p = invoke_python(&cwd);
        #[cfg(windows)]
        assert!(p.ends_with(Path::new(".venv\\Scripts\\python.exe")) || p.ends_with(".venv/Scripts/python.exe") || p.components().any(|c| c.as_os_str() == "python.exe"));
        #[cfg(not(windows))]
        assert!(p.ends_with(".venv/bin/python"));
    }

    #[test]
    fn list_available_shell_commands() {
        let root = tmp();
        let m = TerminalModule::new(
            root.clone(),
            vec![root.to_string_lossy().into_owned()],
            vec!["findstr".into(), "cargo".into()],
            5,
            5,
        );
        let out = m.call("list_available_shell_commands", &Map::new()).unwrap();
        assert!(out.output.contains("findstr"), "{}", out.output);
        assert!(out.output.contains("cargo"), "{}", out.output);
        let _ = fs::remove_dir_all(&root);
    }
}
