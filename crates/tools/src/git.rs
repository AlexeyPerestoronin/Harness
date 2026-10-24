//! Git tool set (`tools.git`): track check, diff, apply patch, discard changes.
//!
//! Every subprocess is `Command::new(executable)` from conf — never a hardcoded
//! `"git"` program name. Timeouts come from `tools.git.execution-limit`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::fs::Sandbox;
use crate::{arg_str, CallKind, CallResult, ToolDescriptor};

static PATCH_SEQ: AtomicU64 = AtomicU64::new(1);

/// Git tools bound to the fs allow-list and a configured executable.
pub struct GitModule {
    sandbox: Arc<Mutex<Sandbox>>,
    executable: String,
    execution_limit: u64,
    cwd: PathBuf,
    temp_dir: PathBuf,
}

impl GitModule {
    /// Construct from conf values and the allow-list owned by `fs`.
    pub fn new(
        sandbox: Arc<Mutex<Sandbox>>,
        executable: String,
        execution_limit: u64,
        cwd: PathBuf,
        temp_dir: PathBuf,
    ) -> Self {
        Self {
            sandbox,
            executable,
            execution_limit,
            cwd,
            temp_dir,
        }
    }

    /// JSON Schema descriptors for the git tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("is_file_under_git", "Check whether a file is tracked by git.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("apply_diff_patch",
                "Apply a unified diff patch to files. The patch MUST strictly follow the standard `git diff` format, including correct file headers (--- and +++) and hunk headers with line numbers and control symbols (e.g., @@ -185,12 +185,6 @@). Do not truncate the patch or skip headers, as it uses `git apply` under the hood and will fail on malformed diffs.",
                json!({"type":"object","properties":{"patch":{"type":"string"}},"required":["patch"]})),
            desc("get_file_diff", "Get the diff for the target file.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("discard_file_changes", "Discard file changes after HEAD.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        ]
    }

    /// Dispatch a named git tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "is_file_under_git" => Ok(CallResult::plain(
                self.is_file_under_git(&arg_str(args, "path")?)?,
            )),
            "get_file_diff" => Ok(CallResult::plain(
                self.get_file_diff(&arg_str(args, "path")?)?,
            )),
            "discard_file_changes" => Ok(CallResult::plain(
                self.discard_file_changes(&arg_str(args, "path")?)?,
            )),
            "apply_diff_patch" => {
                let (message, tmp) = self.apply_diff_patch(&arg_str(args, "patch")?)?;
                Ok(CallResult {
                    output: message,
                    kind: CallKind::Patch { tmp },
                })
            }
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn is_file_under_git(&self, path: &str) -> Result<String, String> {
        (|| {
            self.check(path, 'r')?;
            let result = self.run_git(
                &["ls-files", "--error-unmatch", "--", path],
                &self.cwd,
                None,
            )?;
            if result.status_ok {
                Ok(format!("file '{path}' is under git"))
            } else {
                Ok(format!("file '{path}' is not under git"))
            }
        })()
        .map_err(|e: String| format!("cannot check if file '{path}' is under git → {e}"))
    }

    fn get_file_diff(&self, path: &str) -> Result<String, String> {
        (|| {
            self.check(path, 'r')?;
            let result = self.run_git(&["diff", "HEAD", "--", path], &self.cwd, None)?;
            if !result.status_ok {
                let error_text = result.err_text();
                return Err(if error_text.is_empty() {
                    format!("git diff failed with code {}", result.code)
                } else {
                    error_text
                });
            }
            Ok(result.stdout)
        })()
        .map_err(|e: String| format!("cannot get diff for '{path}' → {e}"))
    }

    fn discard_file_changes(&self, path: &str) -> Result<String, String> {
        (|| {
            self.check(path, 'w')?;
            let result = self.run_git(&["checkout", "HEAD", "--", path], &self.cwd, None)?;
            if !result.status_ok {
                let error_text = result.err_text();
                return Err(if error_text.is_empty() {
                    format!("git checkout failed with code {}", result.code)
                } else {
                    error_text
                });
            }
            Ok(format!("successfully discarded changes in '{path}'"))
        })()
        .map_err(|e: String| format!("cannot discard changes in '{path}' → {e}"))
    }

    fn apply_diff_patch(&self, patch: &str) -> Result<(String, String), String> {
        (|| {
            if patch.trim().is_empty() {
                return Err("invalid or empty patch format".to_string());
            }
            let targets = extract_patch_target_paths(patch);
            if targets.is_empty() {
                return Err("invalid or empty patch format".to_string());
            }
            for rel_path in &targets {
                self.check(rel_path, 'w')?;
            }
            fs::create_dir_all(&self.temp_dir).map_err(|e| e.to_string())?;
            let seq = PATCH_SEQ.fetch_add(1, Ordering::Relaxed);
            let tmp = self.temp_dir.join(format!("harness-{seq}.patch"));
            let mut patch_to_write = patch.to_string();
            if !patch_to_write.ends_with('\n') {
                patch_to_write.push('\n');
            }
            fs::write(&tmp, patch_to_write).map_err(|e| e.to_string())?;
            let tmp_s = tmp.to_string_lossy().into_owned();
            let result = self.run_git(
                &[
                    "apply",
                    "--ignore-whitespace",
                    "--ignore-space-change",
                    "--recount",
                    &tmp_s,
                ],
                &self.cwd,
                None,
            )?;
            if !result.status_ok {
                let error_text = result.err_text();
                return Err(if error_text.is_empty() {
                    format!("git apply failed with code {}", result.code)
                } else {
                    error_text
                });
            }
            Ok((
                format!("successfully applied patch to {}", targets.join(", ")),
                tmp_s,
            ))
        })()
        .map_err(|e: String| format!("cannot apply diff patch → {e}"))
    }

    fn check(&self, path: &str, mode: char) -> Result<(), String> {
        self.sandbox
            .lock()
            .map_err(|e| e.to_string())?
            .check_access(path, mode)
    }

    fn run_git(&self, args: &[&str], cwd: &Path, stdin: Option<&str>) -> Result<GitOut, String> {
        let mut cmd = git_command(&self.executable, args);
        cmd.current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if stdin.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("cannot spawn git executable '{}': {e}", self.executable))?;
        if let Some(input) = stdin {
            use std::io::Write;
            if let Some(mut s) = child.stdin.take() {
                let _ = s.write_all(input.as_bytes());
            }
        }
        let output = wait_with_timeout(child, self.execution_limit)?;
        Ok(GitOut {
            status_ok: output.status.success(),
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

struct GitOut {
    status_ok: bool,
    code: i32,
    stdout: String,
    stderr: String,
}

impl GitOut {
    fn err_text(&self) -> String {
        let t = self.stderr.trim();
        if t.is_empty() {
            self.stdout.trim().to_string()
        } else {
            t.to_string()
        }
    }
}

/// Build a git command using the configured executable (never a hardcoded `"git"`).
pub fn git_command(executable: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(executable);
    cmd.args(args);
    cmd
}

/// Pull target paths out of unified-diff / `git diff` headers.
pub fn extract_patch_target_paths(patch: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut add = |raw: &str| {
        let mut raw = raw.trim().trim_matches('"').to_string();
        if raw.starts_with("a/") || raw.starts_with("b/") {
            raw = raw[2..].to_string();
        }
        if raw.is_empty() || raw == "/dev/null" || seen.contains(&raw) {
            return;
        }
        seen.insert(raw.clone());
        paths.push(raw);
    };
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                add(parts[0]);
                add(parts[1]);
            }
        } else if let Some(rest) = line.strip_prefix("--- ") {
            add(rest.split('\t').next().unwrap_or(rest));
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            add(rest.split('\t').next().unwrap_or(rest));
        }
    }
    paths
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout_secs: u64,
) -> Result<std::process::Output, String> {
    use std::io::Read;
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "execution of git exceed the limit (available limit is {timeout_secs}s)"
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut p) = stdout_pipe.take() {
        let _ = p.read_to_end(&mut stdout);
    }
    if let Some(mut p) = stderr_pipe.take() {
        let _ = p.read_to_end(&mut stderr);
    }
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}
