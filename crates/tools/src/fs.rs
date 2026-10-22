//! Filesystem tool set (`tools.fs`): files/folders, search, and allow-list introspection.
//!
//! This module owns `available-dirs`, `available-file-extensions`, `available-files`,
//! `deprecated-dirs` and `deprecated-files`.
//! `write_file` creates parent directories; `create_file` does not and truncates
//! an existing file.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::{arg_str, CallKind, CallResult, ToolDescriptor};

mod access;
mod search;

pub use access::{abspath, Sandbox};

/// Filesystem tools bound to the allow-list this module owns.
pub struct FsModule {
    sandbox: Arc<Mutex<Sandbox>>,
}

impl FsModule {
    /// Wrap an existing allow-list.
    pub fn new(sandbox: Arc<Mutex<Sandbox>>) -> Self {
        Self { sandbox }
    }

    /// Build the fs module and the allow-list it owns.
    ///
    /// When `settings` is `None` there is no module and the allow-list is empty
    /// (still shared with `git` / `user`).
    pub fn build(settings: Option<&conf::FsSettings>) -> (Option<Self>, Arc<Mutex<Sandbox>>) {
        match settings {
            Some(fs) => {
                let module = Self::new(Arc::new(Mutex::new(Sandbox::from_fs(
                    &fs.available_dirs,
                    &fs.available_files,
                    &fs.available_file_extensions,
                    &fs.deprecated_dirs,
                    &fs.deprecated_files,
                ))));
                let sandbox = module.sandbox.clone();
                (Some(module), sandbox)
            }
            None => (None, Arc::new(Mutex::new(Sandbox::empty()))),
        }
    }

    /// JSON Schema descriptors for the fs tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("read_file", "Read the contents of a file.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("write_file", "Write a file.", json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]})),
            desc("create_file", "Create a file.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("remove_file", "Delete a file.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("move_file", "Move a file.", json!({"type":"object","properties":{"src":{"type":"string"},"dst":{"type":"string"}},"required":["src","dst"]})),
            desc("list_folder", "Return the contents of a directory to the given depth (depth >= 0, where 0 is root contents only).", json!({"type":"object","properties":{"path":{"type":"string"},"depth":{"type":"string"}},"required":["path","depth"]})),
            desc("create_folder", "Create a folder.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("remove_folder", "Delete a folder.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("move_folder", "Move a folder.", json!({"type":"object","properties":{"src":{"type":"string"},"dst":{"type":"string"}},"required":["src","dst"]})),
            desc("find_str", "Search for a string in files starting from the root directory.", json!({"type":"object","properties":{"root":{"type":"string"},"string":{"type":"string"}},"required":["root","string"]})),
            desc("find_str_by_regex", "Search for lines matching a regular expression starting from the root directory.", json!({"type":"object","properties":{"root":{"type":"string"},"regex":{"type":"string"}},"required":["root","regex"]})),
            desc("find_file_by_str", "Find files whose names contain the given string, starting from the root directory.", json!({"type":"object","properties":{"root":{"type":"string"},"string":{"type":"string"}},"required":["root","string"]})),
            desc("find_files_by_regex", "Find files whose names match the given regular expression, starting from the root directory.", json!({"type":"object","properties":{"root":{"type":"string"},"regex":{"type":"string"}},"required":["root","regex"]})),
            desc("list_available_file_extension", "List of available file extensions.", json!({})),
            desc("list_available_r_dir", "List of directories available for reading.", json!({})),
            desc("list_available_w_dir", "List of directories available for writing.", json!({})),
            desc("list_available_r_file", "List of files available for reading.", json!({})),
            desc("list_available_w_file", "List of files available for writing.", json!({})),
        ]
    }

    /// Dispatch a named fs tool.
    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<CallResult, String> {
        match name {
            "read_file" => {
                let text = self.read_file(&arg_str(args, "path")?)?;
                Ok(CallResult {
                    output: text,
                    kind: CallKind::Read,
                })
            }
            "write_file" => {
                let text = self.write_file(&arg_str(args, "path")?, &arg_str(args, "content")?)?;
                Ok(CallResult {
                    output: text,
                    kind: CallKind::Write,
                })
            }
            "create_file" => Ok(CallResult::plain(
                self.create_file(&arg_str(args, "path")?)?,
            )),
            "remove_file" => Ok(CallResult::plain(
                self.remove_file(&arg_str(args, "path")?)?,
            )),
            "move_file" => Ok(CallResult::plain(
                self.move_file(&arg_str(args, "src")?, &arg_str(args, "dst")?)?,
            )),
            "list_folder" => Ok(CallResult::plain(
                self.list_folder(&arg_str(args, "path")?, &arg_str(args, "depth")?)?,
            )),
            "create_folder" => Ok(CallResult::plain(
                self.create_folder(&arg_str(args, "path")?)?,
            )),
            "remove_folder" => Ok(CallResult::plain(
                self.remove_folder(&arg_str(args, "path")?)?,
            )),
            "move_folder" => Ok(CallResult::plain(
                self.move_folder(&arg_str(args, "src")?, &arg_str(args, "dst")?)?,
            )),
            "find_str" => Ok(CallResult::plain(search::find_str(
                &self.sandbox,
                &arg_str(args, "root")?,
                &arg_str(args, "string")?,
            )?)),
            "find_str_by_regex" => Ok(CallResult::plain(search::find_str_by_regex(
                &self.sandbox,
                &arg_str(args, "root")?,
                &arg_str(args, "regex")?,
            )?)),
            "find_file_by_str" => Ok(CallResult::plain(search::find_file_by_str(
                &self.sandbox,
                &arg_str(args, "root")?,
                &arg_str(args, "string")?,
            )?)),
            "find_files_by_regex" => Ok(CallResult::plain(search::find_files_by_regex(
                &self.sandbox,
                &arg_str(args, "root")?,
                &arg_str(args, "regex")?,
            )?)),
            "list_available_file_extension" => Ok(CallResult::plain(self.list_extensions()?)),
            "list_available_r_dir" => Ok(CallResult::plain(self.list_paths(|sb| sb.r_dirs())?)),
            "list_available_w_dir" => Ok(CallResult::plain(self.list_paths(|sb| sb.w_dirs())?)),
            "list_available_r_file" => Ok(CallResult::plain(self.list_paths(|sb| sb.r_files())?)),
            "list_available_w_file" => Ok(CallResult::plain(self.list_paths(|sb| sb.w_files())?)),
            _ => Err(format!("tool '{name}' not available")),
        }
    }

    fn read_file(&self, path: &str) -> Result<String, String> {
        self.guard(path, 'r', true)
            .and_then(|_| fs::read_to_string(path).map_err(|e| e.to_string()))
            .map_err(|e| format!("cannot read file '{path}' → {e}"))
    }

    fn write_file(&self, path: &str, content: &str) -> Result<String, String> {
        (|| {
            self.guard(path, 'w', true)?;
            if let Some(parent) = Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            fs::write(path, content).map_err(|e| e.to_string())?;
            Ok(format!("success: wrote {path}"))
        })()
        .map_err(|e: String| format!("cannot write file '{path}' → {e}"))
    }

    fn create_file(&self, path: &str) -> Result<String, String> {
        (|| {
            self.guard(path, 'w', true)?;
            fs::write(path, "").map_err(|e| e.to_string())?;
            Ok(format!("success: created {path}"))
        })()
        .map_err(|e: String| format!("cannot create file '{path}' → {e}"))
    }

    fn remove_file(&self, path: &str) -> Result<String, String> {
        (|| {
            self.guard(path, 'w', false)?;
            fs::remove_file(path).map_err(|e| e.to_string())?;
            Ok(format!("success: removed {path}"))
        })()
        .map_err(|e: String| format!("cannot remove file '{path}' → {e}"))
    }

    fn move_file(&self, src: &str, dst: &str) -> Result<String, String> {
        (|| {
            self.guard(src, 'w', true)?;
            self.guard(dst, 'w', true)?;
            fs::rename(src, dst).map_err(|e| e.to_string())?;
            Ok(format!("success: moved {src} to {dst}"))
        })()
        .map_err(|e: String| format!("cannot move file '{src}' to {dst} → {e}"))
    }

    fn list_folder(&self, path: &str, depth: &str) -> Result<String, String> {
        (|| {
            let depth: i32 = depth.parse().map_err(|e| format!("{e}"))?;
            let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
            sb.check_access(path, 'r')?;
            Ok(walk_folder(path, depth, &sb).join("\n"))
        })()
        .map_err(|e: String| format!("cannot list folder '{path}' → {e}"))
    }

    fn create_folder(&self, path: &str) -> Result<String, String> {
        (|| {
            self.guard(path, 'w', false)?;
            fs::create_dir_all(path).map_err(|e| e.to_string())?;
            Ok(format!("success: created {path}"))
        })()
        .map_err(|e: String| format!("cannot create folder '{path}' → {e}"))
    }

    fn remove_folder(&self, path: &str) -> Result<String, String> {
        (|| {
            self.guard(path, 'w', false)?;
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
            Ok(format!("success: removed {path}"))
        })()
        .map_err(|e: String| format!("cannot remove folder '{path}' → {e}"))
    }

    fn move_folder(&self, src: &str, dst: &str) -> Result<String, String> {
        (|| {
            self.guard(src, 'w', false)?;
            self.guard(dst, 'w', false)?;
            fs::rename(src, dst).map_err(|e| e.to_string())?;
            Ok(format!("success: moved {src} to {dst}"))
        })()
        .map_err(|e: String| format!("cannot move folder '{src}' to '{dst}' → {e}"))
    }

    fn guard(&self, path: &str, mode: char, check_ext: bool) -> Result<(), String> {
        let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
        sb.check_access(path, mode)?;
        if check_ext {
            sb.check_extensions(path, mode)?;
        }
        Ok(())
    }

    fn list_extensions(&self) -> Result<String, String> {
        let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
        if sb.extensions().is_empty() {
            Ok("available: all".to_string())
        } else {
            Ok(format!("available: {:?}", sb.extensions()))
        }
    }

    fn list_paths(&self, pick: fn(&Sandbox) -> &[String]) -> Result<String, String> {
        let sb = self.sandbox.lock().map_err(|e| e.to_string())?;
        Ok(format!("available: {:?}", pick(&sb)))
    }
}

fn walk_folder(path: &str, depth: i32, sandbox: &Sandbox) -> Vec<String> {
    let mut result = Vec::new();
    if depth <= 0 {
        return result;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return result;
    };
    let mut names: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    names.sort();
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for full in names {
        let s = full.to_string_lossy();
        if sandbox.is_deprecated(&s) {
            continue;
        }
        if full.is_dir() {
            dirs.push(full);
        } else {
            files.push(full);
        }
    }
    for folder in dirs {
        result.push(folder.to_string_lossy().into_owned());
        result.extend(walk_folder(&folder.to_string_lossy(), depth - 1, sandbox));
    }
    for file in files {
        result.push(file.to_string_lossy().into_owned());
    }
    result
}

fn desc(name: &str, description: &str, parameters: Value) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}
