//! Filesystem tool set (`tools.fs`): files/folders, search, and allow-list introspection.
//!
//! This module owns `available-dirs`, `available-file-extensions` and `extra-scope`.
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
                    &fs.extra_scope,
                    &fs.available_file_extensions,
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
        (root, FsModule::new(Arc::new(Mutex::new(sb))))
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
            "find_str",
            "find_str_by_regex",
            "find_file_by_str",
            "find_files_by_regex",
            "list_available_file_extension",
            "list_available_r_dir",
            "list_available_w_dir",
            "list_available_r_file",
            "list_available_w_file",
        ] {
            assert!(names.contains(&n.to_string()), "{n}");
        }
        assert!(!names.contains(&"list_available_shell_commands".to_string()));
    }

    #[test]
    fn find_file_by_name_not_content() {
        let (root, fsmod) = setup();
        fs::write(root.join("needle.txt"), "haystack content").unwrap();
        fs::write(root.join("other.txt"), "needle in content").unwrap();
        fs::create_dir_all(root.join("needle_dir")).unwrap();

        let by_name = fsmod
            .call(
                "find_file_by_str",
                &args(&[("root", &root.to_string_lossy()), ("string", "needle")]),
            )
            .unwrap()
            .output;
        assert!(by_name.contains("needle.txt"), "{by_name}");
        assert!(!by_name.contains("other.txt"), "{by_name}");
        assert!(!by_name.contains("needle_dir"), "{by_name}");

        let by_content = fsmod
            .call(
                "find_file_by_str",
                &args(&[("root", &root.to_string_lossy()), ("string", "haystack")]),
            )
            .unwrap()
            .output;
        assert_eq!(by_content, "");

        let re = fsmod
            .call(
                "find_files_by_regex",
                &args(&[
                    ("root", &root.to_string_lossy()),
                    ("regex", r"needle.*\.txt"),
                ]),
            )
            .unwrap()
            .output;
        assert!(re.contains("needle.txt"), "{re}");
        assert!(!re.contains("other.txt"), "{re}");

        let err = fsmod
            .call(
                "find_files_by_regex",
                &args(&[("root", &root.to_string_lossy()), ("regex", "[unclosed")]),
            )
            .unwrap_err();
        assert!(err.contains("cannot find_files_by_regex"), "{err}");

        let denied = fsmod
            .call(
                "find_file_by_str",
                &args(&[("root", "/no/such/outside"), ("string", "x")]),
            )
            .unwrap_err();
        assert!(denied.contains("cannot find_file_by_str"), "{denied}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn grant_write_visible_in_list_available() {
        let sb = Arc::new(Mutex::new(Sandbox::empty()));
        let m = FsModule::new(sb.clone());
        sb.lock().unwrap().grant_path_access("notes.md", 'w');
        let w = m.call("list_available_w_file", &Map::new()).unwrap();
        assert!(w.output.contains("notes.md"), "{}", w.output);
        let r = m.call("list_available_r_file", &Map::new()).unwrap();
        assert!(r.output.contains("notes.md"), "{}", r.output);
        let ext = m.call("list_available_file_extension", &Map::new()).unwrap();
        assert_eq!(ext.output, "available: all");
    }

    #[test]
    fn lists_follow_allow_list() {
        let sb = Sandbox::from_fs(
            &["w:src".into(), "r:docs".into()],
            &["r:README.md".into()],
            &[".md".into()],
        );
        let m = FsModule::new(Arc::new(Mutex::new(sb)));
        let ext = m.call("list_available_file_extension", &Map::new()).unwrap();
        assert!(ext.output.contains(".md"), "{}", ext.output);
        let rdir = m.call("list_available_r_dir", &Map::new()).unwrap();
        assert!(rdir.output.contains("src"), "{}", rdir.output);
        assert!(rdir.output.contains("docs"), "{}", rdir.output);
        let wdir = m.call("list_available_w_dir", &Map::new()).unwrap();
        assert!(wdir.output.contains("src"), "{}", wdir.output);
        assert!(!wdir.output.contains("docs"), "{}", wdir.output);
    }
}
