//! Filesystem tool set (`tools.fs`): read/write/create/remove/move for files and folders.
//!
//! Access is gated by the shared sandbox. `write_file` creates parent directories;
//! `create_file` does not and truncates an existing file.

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::sandbox::Sandbox;
use crate::{arg_str, CallKind, CallResult, ToolDescriptor};

/// Filesystem tools bound to a shared sandbox.
pub struct FsModule {
    sandbox: Arc<Mutex<Sandbox>>,
}

impl FsModule {
    /// Wrap an existing sandbox.
    pub fn new(sandbox: Arc<Mutex<Sandbox>>) -> Self {
        Self { sandbox }
    }

    /// JSON Schema descriptors for the fs tools.
    pub fn descriptors() -> Vec<ToolDescriptor> {
        vec![
            desc("read_file", "Прочитать содержимое файла.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("write_file", "Записать файл.", json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]})),
            desc("create_file", "Создать файл.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("remove_file", "Удалить файл.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("move_file", "Переместить файл.", json!({"type":"object","properties":{"src":{"type":"string"},"dst":{"type":"string"}},"required":["src","dst"]})),
            desc("list_folder", "Возвращает содержимое директории на заданную глубину (depth >= 0, где 0 - только корневое содержимое).", json!({"type":"object","properties":{"path":{"type":"string"},"depth":{"type":"string"}},"required":["path","depth"]})),
            desc("create_folder", "Создать папку.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("remove_folder", "Удалить папку.", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
            desc("move_folder", "Переместить папку.", json!({"type":"object","properties":{"src":{"type":"string"},"dst":{"type":"string"}},"required":["src","dst"]})),
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
            "create_file" => Ok(CallResult::plain(self.create_file(&arg_str(args, "path")?)?)),
            "remove_file" => Ok(CallResult::plain(self.remove_file(&arg_str(args, "path")?)?)),
            "move_file" => Ok(CallResult::plain(
                self.move_file(&arg_str(args, "src")?, &arg_str(args, "dst")?)?,
            )),
            "list_folder" => Ok(CallResult::plain(
                self.list_folder(&arg_str(args, "path")?, &arg_str(args, "depth")?)?,
            )),
            "create_folder" => {
                Ok(CallResult::plain(self.create_folder(&arg_str(args, "path")?)?))
            }
            "remove_folder" => {
                Ok(CallResult::plain(self.remove_folder(&arg_str(args, "path")?)?))
            }
            "move_folder" => Ok(CallResult::plain(
                self.move_folder(&arg_str(args, "src")?, &arg_str(args, "dst")?)?,
            )),
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
            self.guard(path, 'r', false)?;
            Ok(walk_folder(path, depth).join("\n"))
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
}

fn walk_folder(path: &str, depth: i32) -> Vec<String> {
    let mut result = Vec::new();
    if depth <= 0 {
        return result;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return result;
    };
    let mut names: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    names.sort();
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for full in names {
        if full.is_dir() {
            dirs.push(full);
        } else {
            files.push(full);
        }
    }
    for folder in dirs {
        result.push(folder.to_string_lossy().into_owned());
        result.extend(walk_folder(&folder.to_string_lossy(), depth - 1));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Sandbox;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn setup() -> (PathBuf, FsModule) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("fs-mod-{nanos}"));
        fs::create_dir_all(&root).unwrap();
        let sb = Sandbox::from_fs(&[format!("w:{}", root.display())], &[], &[]);
        (
            root,
            FsModule::new(Arc::new(Mutex::new(sb))),
        )
    }

    fn args(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), Value::String((*v).to_string())))
            .collect()
    }

    #[test]
    fn write_creates_parents_create_does_not() {
        let (root, fsmod) = setup();
        let nested = root.join("a").join("b.txt");
        let out = fsmod
            .call(
                "write_file",
                &args(&[
                    ("path", &nested.to_string_lossy()),
                    ("content", "hi"),
                ]),
            )
            .unwrap();
        assert!(out.output.contains("success: wrote"));
        assert_eq!(fs::read_to_string(&nested).unwrap(), "hi");

        let missing = root.join("nope").join("c.txt");
        let err = fsmod
            .call("create_file", &args(&[("path", &missing.to_string_lossy())]))
            .unwrap_err();
        assert!(err.contains("cannot create file"), "{err}");

        let existing = root.join("e.txt");
        fs::write(&existing, "old").unwrap();
        fsmod
            .call("create_file", &args(&[("path", &existing.to_string_lossy())]))
            .unwrap();
        assert_eq!(fs::read_to_string(&existing).unwrap(), "");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_folder_depth_zero_and_order() {
        let (root, fsmod) = setup();
        fs::create_dir_all(root.join("d")).unwrap();
        fs::write(root.join("a.txt"), "x").unwrap();
        fs::write(root.join("d").join("b.txt"), "y").unwrap();
        let empty = fsmod
            .call(
                "list_folder",
                &args(&[("path", &root.to_string_lossy()), ("depth", "0")]),
            )
            .unwrap();
        assert_eq!(empty.output, "");
        let listing = fsmod
            .call(
                "list_folder",
                &args(&[("path", &root.to_string_lossy()), ("depth", "2")]),
            )
            .unwrap()
            .output;
        let lines: Vec<_> = listing.lines().collect();
        assert!(lines[0].ends_with("d") || lines[0].contains("d"));
        assert!(listing.contains("a.txt"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn descriptors_cover_fs_tools() {
        let names: Vec<_> = FsModule::descriptors().into_iter().map(|d| d.name).collect();
        for n in [
            "read_file",
            "write_file",
            "create_file",
            "remove_file",
            "move_file",
            "list_folder",
            "create_folder",
            "remove_folder",
            "move_folder",
        ] {
            assert!(names.contains(&n.to_string()), "{n}");
        }
    }
}
