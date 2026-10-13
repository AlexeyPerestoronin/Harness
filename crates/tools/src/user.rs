//! User-interaction tool set (`tools.user`): questions and runtime access grants.
//!
//! Confirmations always go through stdin (`y` / `yes` / `д` / `да`). Path grants
//! mutate the shared sandbox. `request_command_shell_execution` runs the command
//! once on approval and does not add it to the terminal whitelist.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::process::run_observed_command;
use crate::sandbox::Sandbox;
use crate::{arg_str, CallResult, ToolDescriptor};

/// User-interaction tools.
pub struct UserModule {
    sandbox: Arc<Mutex<Sandbox>>,
    command_timeout: Option<u64>,
}

impl UserModule {
    /// Bind to the shared sandbox. `command_timeout` comes from terminal settings
    /// when that module is enabled.
    pub fn new(sandbox: Arc<Mutex<Sandbox>>, command_timeout: Option<u64>) -> Self {
        Self {
            sandbox,
            command_timeout,
        }
    }

    /// JSON Schema descriptors for the user tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("ask_user", "Задать вопрос пользователю и получить ответ.", json!({"type":"object","properties":{"question":{"type":"string"}},"required":["question"]})),
            desc("request_read_access_for", "Запрос у пользователя read-доступа к папке/файлу.", json!({"type":"object","properties":{"reason":{"type":"string"},"path":{"type":"string"}},"required":["reason","path"]})),
            desc("request_write_access_for", "Запрос у пользователя write-доступа к папке/файлу.", json!({"type":"object","properties":{"reason":{"type":"string"},"path":{"type":"string"}},"required":["reason","path"]})),
            desc("request_command_shell_execution", "Запрос у пользователя исполнения shell-команды.", json!({"type":"object","properties":{"reason":{"type":"string"},"command":{"type":"string"},"cwd":{"type":"string"}},"required":["reason","command","cwd"]})),
        ]
    }

    /// Dispatch a named user tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "ask_user" => Ok(CallResult::plain(self.ask_user(&arg_str(args, "question")?)?)),
            "request_read_access_for" => Ok(CallResult::plain(self.request_read_access_for(
                &arg_str(args, "reason")?,
                &arg_str(args, "path")?,
            )?)),
            "request_write_access_for" => Ok(CallResult::plain(self.request_write_access_for(
                &arg_str(args, "reason")?,
                &arg_str(args, "path")?,
            )?)),
            "request_command_shell_execution" => Ok(CallResult::plain(
                self.request_command_shell_execution(
                    &arg_str(args, "reason")?,
                    &arg_str(args, "command")?,
                    &arg_str(args, "cwd")?,
                )?,
            )),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn ask_user(&self, question: &str) -> Result<String, String> {
        prompt_line(&format!("{question}\n")).map_err(|e| format!("cannot ask user → {e}"))
    }

    fn request_read_access_for(&self, reason: &str, path: &str) -> Result<String, String> {
        (|| {
            match request_permission(&format!(
                "Запрос read-доступа к '{path}'\nПричина: {reason}\nРазрешить? [y/n]"
            ))? {
                None => {
                    self.sandbox
                        .lock()
                        .map_err(|e| e.to_string())?
                        .grant_path_access(path, 'r');
                    Ok(format!("read-access granted to '{path}'"))
                }
                Some(denial) => Ok(format!("read-access denied to '{path}': {denial}")),
            }
        })()
        .map_err(|e: String| format!("cannot request read access for '{path}' → {e}"))
    }

    fn request_write_access_for(&self, reason: &str, path: &str) -> Result<String, String> {
        (|| {
            match request_permission(&format!(
                "Запрос write-доступа к '{path}'\nПричина: {reason}\nРазрешить? [y/n]"
            ))? {
                None => {
                    self.sandbox
                        .lock()
                        .map_err(|e| e.to_string())?
                        .grant_path_access(path, 'w');
                    Ok(format!("write-access granted to '{path}'"))
                }
                Some(denial) => Ok(format!("write-access denied to '{path}': {denial}")),
            }
        })()
        .map_err(|e: String| format!("cannot request write access for '{path}' → {e}"))
    }

    fn request_command_shell_execution(
        &self,
        reason: &str,
        command: &str,
        cwd: &str,
    ) -> Result<String, String> {
        (|| {
            match request_permission(&format!(
                "Запрос исполнения команды\nКоманда: {command}\nКаталог: {cwd}\nПричина: {reason}\nРазрешить? [y/n]"
            ))? {
                None => {
                    let timeout = self.command_timeout.ok_or_else(|| {
                        "command-execution-limit is not configured (enable tools.terminal)".to_string()
                    })?;
                    run_observed_command(command, Path::new(cwd), timeout)
                }
                Some(denial) => Ok(format!("command execution denied: {denial}")),
            }
        })()
        .map_err(|e: String| format!("cannot request command shell execution → {e}"))
    }
}

fn request_permission(prompt: &str) -> Result<Option<String>, String> {
    let answer = prompt_line(&format!("{prompt}\n"))?;
    if is_yes(&answer) {
        Ok(None)
    } else {
        Ok(Some(prompt_line("Причина отказа:\n")?))
    }
}

fn is_yes(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes" | "д" | "да")
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    io::stdin()
        .read_line(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(buf)
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
    use crate::sandbox::Sandbox;

    #[test]
    fn descriptors_registered() {
        let names: Vec<_> = UserModule::descriptors().into_iter().map(|d| d.name).collect();
        for n in [
            "ask_user",
            "request_read_access_for",
            "request_write_access_for",
            "request_command_shell_execution",
        ] {
            assert!(names.contains(&n.to_string()));
        }
    }

    #[test]
    fn yes_tokens() {
        assert!(is_yes("y"));
        assert!(is_yes("YES"));
        assert!(is_yes("д"));
        assert!(is_yes("ДА"));
        assert!(!is_yes("n"));
        assert!(!is_yes(""));
    }

    #[test]
    fn grant_updates_sandbox() {
        let sb = Arc::new(Mutex::new(Sandbox::empty()));
        sb.lock().unwrap().grant_path_access("notes.md", 'w');
        let g = sb.lock().unwrap();
        assert!(g.r_files().iter().any(|p| p == "notes.md"));
        assert!(g.w_files().iter().any(|p| p == "notes.md"));
    }
}
