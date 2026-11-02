//! User-interaction tool set (`tools.user`): questions and runtime access grants.
//!
//! Confirmations go through the [`Interaction`] contract (`y` / `yes` / `д` / `да`).
//! Path grants mutate the allow-list owned by `fs`.
//! `request_command_shell_execution` runs the command once on approval and does
//! not add it to the terminal whitelist.

use std::sync::{Arc, Mutex};

use interactions::{Interaction, InteractionKind, InteractionRequest};
use serde_json::{json, Map, Value};

use crate::fs::Sandbox;
use crate::process::run_observed_command;
use crate::{arg_str, CallResult, ToolDescriptor};

/// User-interaction tools.
#[derive(Clone)]
pub struct UserModule {
    sandbox: Arc<Mutex<Sandbox>>,
    command_timeout: Option<u64>,
    interaction: Arc<dyn Interaction>,
}

impl UserModule {
    /// Bind to the allow-list owned by `fs`. `command_timeout` comes from terminal settings
    /// when that module is enabled.
    pub fn new(
        sandbox: Arc<Mutex<Sandbox>>,
        command_timeout: Option<u64>,
        interaction: Arc<dyn Interaction>,
    ) -> Self {
        Self {
            sandbox,
            command_timeout,
            interaction,
        }
    }

    /// JSON Schema descriptors for the user tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc(
                "ask_user",
                "Ask the user a question and get the answer.",
                json!({"type":"object","properties":{"question":{"type":"string"}},"required":["question"]}),
            ),
            desc(
                "request_read_access_for",
                "Request read access to a folder/file from the user.",
                json!({"type":"object","properties":{"reason":{"type":"string"},"path":{"type":"string"}},"required":["reason","path"]}),
            ),
            desc(
                "request_write_access_for",
                "Request write access to a folder/file from the user.",
                json!({"type":"object","properties":{"reason":{"type":"string"},"path":{"type":"string"}},"required":["reason","path"]}),
            ),
            desc(
                "request_command_shell_execution",
                "Request permission from the user to execute a shell command.",
                json!({"type":"object","properties":{"reason":{"type":"string"},"command":{"type":"string"},"cwd":{"type":"string"}},"required":["reason","command","cwd"]}),
            ),
        ]
    }

    /// Dispatch a named user tool.
    pub async fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "ask_user" => Ok(CallResult::plain(
                self.ask_user(&arg_str(args, "question")?).await?,
            )),
            "request_read_access_for" => Ok(CallResult::plain(
                self.request_read_access_for(&arg_str(args, "reason")?, &arg_str(args, "path")?)
                    .await?,
            )),
            "request_write_access_for" => Ok(CallResult::plain(
                self.request_write_access_for(&arg_str(args, "reason")?, &arg_str(args, "path")?)
                    .await?,
            )),
            "request_command_shell_execution" => {
                Ok(CallResult::plain(self.request_command_shell_execution(
                    &arg_str(args, "reason")?,
                    &arg_str(args, "command")?,
                    &arg_str(args, "cwd")?,
                ).await?))
            }
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    async fn ask_user(&self, question: &str) -> Result<String, String> {
        self.input_text(&format!("{question}\n"))
            .await
            .map_err(|e| format!("cannot ask user → {e}"))
    }

    async fn request_read_access_for(&self, reason: &str, path: &str) -> Result<String, String> {
        self.request_read_access_for_inner(reason, path)
            .await
            .map_err(|e| format!("cannot request read access for '{path}' → {e}"))
    }

    async fn request_read_access_for_inner(
        &self,
        reason: &str,
        path: &str,
    ) -> Result<String, String> {
        self.resolve_in_root(path)?;
        if let Some(denial) = self.deprecated_grant_denial(path, 'r')? {
            return Ok(denial);
        }
        let prompt = format!(
            "Запрос read-доступа к '{path}'\nПричина: {reason}\nРазрешить? [y/n]"
        );
        match self.confirm_permission(prompt).await? {
            None => {
                self.grant_path_access(path, 'r')?;
                Ok(format!("read-access granted to '{path}'"))
            }
            Some(denial) => Ok(format!("read-access denied to '{path}': {denial}")),
        }
    }

    async fn request_write_access_for(&self, reason: &str, path: &str) -> Result<String, String> {
        self.request_write_access_for_inner(reason, path)
            .await
            .map_err(|e| format!("cannot request write access for '{path}' → {e}"))
    }

    async fn request_write_access_for_inner(
        &self,
        reason: &str,
        path: &str,
    ) -> Result<String, String> {
        self.resolve_in_root(path)?;
        if let Some(denial) = self.deprecated_grant_denial(path, 'w')? {
            return Ok(denial);
        }
        let prompt = format!(
            "Запрос write-доступа к '{path}'\nПричина: {reason}\nРазрешить? [y/n]"
        );
        match self.confirm_permission(prompt).await? {
            None => {
                self.grant_path_access(path, 'w')?;
                Ok(format!("write-access granted to '{path}'"))
            }
            Some(denial) => Ok(format!("write-access denied to '{path}': {denial}")),
        }
    }

    async fn request_command_shell_execution(
        &self,
        reason: &str,
        command: &str,
        cwd: &str,
    ) -> Result<String, String> {
        self.request_command_shell_execution_inner(reason, command, cwd)
            .await
            .map_err(|e| format!("cannot request command shell execution → {e}"))
    }

    async fn request_command_shell_execution_inner(
        &self,
        reason: &str,
        command: &str,
        cwd: &str,
    ) -> Result<String, String> {
        let run_cwd = self
            .sandbox
            .lock()
            .map_err(|e| e.to_string())?
            .resolve_in_root(cwd)?;
        let prompt = format!(
            "Запрос исполнения команды\nКоманда: {command}\nКаталог: {cwd}\nПричина: {reason}\nРазрешить? [y/n]"
        );
        match self.confirm_permission(prompt).await? {
            None => {
                let timeout = self.command_timeout.ok_or_else(|| {
                    "command-execution-limit is not configured (enable tools.terminal)".to_string()
                })?;
                let command = command.to_string();
                tokio::task::spawn_blocking(move || {
                    run_observed_command(&command, &run_cwd, timeout)
                })
                .await
                .map_err(|e| e.to_string())?
            }
            Some(denial) => Ok(format!("command execution denied: {denial}")),
        }
    }

    async fn confirm_permission(&self, prompt: String) -> Result<Option<String>, String> {
        let answer = self
            .input_confirm(&prompt)
            .await
            .map_err(|e| e.to_string())?;
        if is_yes(&answer) {
            Ok(None)
        } else {
            let reason = self.input_text("Причина отказа:\n").await?;
            Ok(Some(reason))
        }
    }

    async fn input_text(&self, prompt: &str) -> Result<String, String> {
        let request = InteractionRequest {
            kind: InteractionKind::Text,
            prompt: prompt.to_string(),
            sensitive: false,
        };
        self.interaction
            .input(&request, true)
            .await
            .map_err(|e| e.to_string())
    }

    async fn input_confirm(&self, prompt: &str) -> Result<String, String> {
        let request = InteractionRequest {
            kind: InteractionKind::Confirm,
            prompt: prompt.to_string(),
            sensitive: false,
        };
        self.interaction
            .input(&request, true)
            .await
            .map_err(|e| e.to_string())
    }

    fn resolve_in_root(&self, path: &str) -> Result<(), String> {
        self.sandbox
            .lock()
            .map_err(|e| e.to_string())?
            .resolve_in_root(path)?;
        Ok(())
    }

    fn deprecated_grant_denial(&self, path: &str, mode: char) -> Result<Option<String>, String> {
        let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
        Ok(deprecated_grant_denial_locked(&sb, path, mode))
    }

    fn grant_path_access(&self, path: &str, mode: char) -> Result<(), String> {
        self.sandbox
            .lock()
            .map_err(|e| e.to_string())?
            .grant_path_access(path, mode)
    }
}

/// Refusal before prompting the user when the path is excluded by `deprecated-*`.
fn deprecated_grant_denial_locked(sandbox: &Sandbox, path: &str, mode: char) -> Option<String> {
    if sandbox.is_deprecated(path) {
        let kind = if mode == 'w' { "write" } else { "read" };
        Some(format!(
            "{kind}-access denied to '{path}': path is deprecated"
        ))
    } else {
        None
    }
}

fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_lowercase().as_str(),
        "y" | "yes" | "д" | "да"
    )
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

