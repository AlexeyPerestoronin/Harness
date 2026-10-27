//! Allow-lists and denylists for directories, exact files and extensions, owned by `fs`.
//!
//! Directories are matched by `abspath` prefix; available-files by exact
//! `abspath`. A `w:` grant always implies `r:`. Empty extension list means any
//! extension is allowed. The lists are mutated at runtime by `request_*`.
//!
//! `deprecated-dirs` / `deprecated-files` are checked before the allow-list.
//! Deprecated directories match by path-component prefix (`target` does not
//! block `target2`); deprecated files match exact `abspath`. Windows is case-insensitive.

use std::path::{Component, Path, PathBuf};

/// Allow-lists for directories, exact files and extensions.
#[derive(Debug, Clone, Default)]
pub struct Sandbox {
    /// Root CWD; every path accepted by this sandbox must stay inside it.
    root: PathBuf,
    r_dirs: Vec<String>,
    w_dirs: Vec<String>,
    r_files: Vec<String>,
    w_files: Vec<String>,
    extensions: Vec<String>,
    deprecated_dirs: Vec<String>,
    deprecated_files: Vec<String>,
}

impl Sandbox {
    /// Build from `tools.fs` entries. Unknown / unprefixed items are skipped.
    pub fn from_fs(
        root: PathBuf,
        available_dirs: &[String],
        available_files: &[String],
        extensions: &[String],
        deprecated_dirs: &[String],
        deprecated_files: &[String],
    ) -> Self {
        let mut sb = Self {
            root: normalize_root(&root),
            extensions: extensions.to_vec(),
            deprecated_dirs: deprecated_dirs.to_vec(),
            deprecated_files: deprecated_files.to_vec(),
            r_dirs: Vec::new(),
            w_dirs: Vec::new(),
            r_files: Vec::new(),
            w_files: Vec::new(),
        };
        split_prefixed(available_dirs, &mut sb.r_dirs, &mut sb.w_dirs);
        split_prefixed(available_files, &mut sb.r_files, &mut sb.w_files);
        sb
    }

    /// Empty allow-list (no `fs` module in conf).
    pub fn empty() -> Self {
        Self::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Empty allow-list rooted at `root`.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: normalize_root(&root),
            r_dirs: Vec::new(),
            w_dirs: Vec::new(),
            r_files: Vec::new(),
            w_files: Vec::new(),
            extensions: Vec::new(),
            deprecated_dirs: Vec::new(),
            deprecated_files: Vec::new(),
        }
    }

    /// Root CWD used by this sandbox.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `path` against the sandbox root and reject paths outside it.
    pub fn resolve_in_root(&self, path: &str) -> Result<PathBuf, String> {
        resolve_in_root(&self.root, path)
    }

    /// Read-directory allow-list as stored (including `w` dirs).
    pub fn r_dirs(&self) -> &[String] {
        &self.r_dirs
    }

    /// Write-directory allow-list as stored.
    pub fn w_dirs(&self) -> &[String] {
        &self.w_dirs
    }

    /// Read-file available-files as stored (including `w` files).
    pub fn r_files(&self) -> &[String] {
        &self.r_files
    }

    /// Write-file available-files as stored.
    pub fn w_files(&self) -> &[String] {
        &self.w_files
    }

    /// Allowed extensions. Empty = any.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// Check directory/file access for `r` or `w`.
    pub fn check_access(&self, path: &str, mode: char) -> Result<(), String> {
        let abs_path = self.resolve_in_root(path)?;
        if self.is_deprecated_abs(&abs_path) {
            return Err(format!(
                "{mode}-access denied to {path} → path is deprecated (excluded by 'deprecated-dirs'/'deprecated-files' in tools.fs; do not use it)"
            ));
        }
        let (dirs, files) = if mode == 'w' {
            (&self.w_dirs, &self.w_files)
        } else {
            (&self.r_dirs, &self.r_files)
        };
        if self.is_allowed_abs(&abs_path, dirs, files) {
            Ok(())
        } else {
            Err(format!(
                "{mode}-access denied to {path} → unavailable file location (list of available directories for {mode}-access is {dirs:?}; list of available files for {mode}-access is {files:?})"
            ))
        }
    }

    /// Check directory/file access with a custom error message.
    pub fn check_access_msg(&self, path: &str, mode: char, error: &str) -> Result<(), String> {
        self.check_access(path, mode).map_err(|_| error.to_string())
    }

    /// Check file extension against the allow-list.
    pub fn check_extensions(&self, path: &str, mode: char) -> Result<(), String> {
        if self.extensions.is_empty() {
            return Ok(());
        }
        let ext = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{s}"))
            .unwrap_or_default();
        if self.extensions.iter().any(|e| e == &ext) {
            Ok(())
        } else {
            Err(format!(
                "{mode}-access denied to {path} → unavailable file extension {ext} (list of available extensions is {:?})",
                self.extensions
            ))
        }
    }

    /// Grant runtime access. Existing directory → prefix list; otherwise available-files.
    /// `w` also adds the path to the matching `r` list.
    /// Deprecated paths are refused and not added to the allow-list.
    pub fn grant_path_access(&mut self, path: &str, mode: char) -> Result<(), String> {
        let abs_path = self.resolve_in_root(path)?;
        if self.is_deprecated_abs(&abs_path) {
            return Err(format!(
                "{mode}-access denied to '{path}': path is deprecated"
            ));
        }
        let is_dir = abs_path.is_dir();
        let r_list = if is_dir {
            &mut self.r_dirs
        } else {
            &mut self.r_files
        };
        if !r_list.iter().any(|p| p == path) {
            r_list.push(path.to_string());
        }
        if mode == 'w' {
            let w_list = if is_dir {
                &mut self.w_dirs
            } else {
                &mut self.w_files
            };
            if !w_list.iter().any(|p| p == path) {
                w_list.push(path.to_string());
            }
        }
        Ok(())
    }

    /// `true` when `path` is a deprecated file or lies inside a deprecated directory.
    pub fn is_deprecated(&self, path: &str) -> bool {
        self.resolve_in_root(path)
            .map(|abs_path| self.is_deprecated_abs(&abs_path))
            .unwrap_or(false)
    }

    fn is_deprecated_abs(&self, path: &Path) -> bool {
        let path_parts = comparable_parts(path);
        if self.deprecated_files.iter().any(|f| {
            self.resolve_in_root(f)
                .map(|abs_f| comparable_parts(&abs_f) == path_parts)
                .unwrap_or(false)
        }) {
            return true;
        }
        self.deprecated_dirs.iter().any(|d| {
            self.resolve_in_root(d)
                .map(|abs_d| {
                    let dir_parts = comparable_parts(&abs_d);
                    !dir_parts.is_empty() && path_parts.starts_with(&dir_parts)
                })
                .unwrap_or(false)
        })
    }

    fn is_allowed_abs(&self, path: &Path, dirs: &[String], files: &[String]) -> bool {
        let path_parts = comparable_parts(path);
        dirs.iter().any(|d| {
            self.resolve_in_root(d)
                .map(|abs_d| {
                    let dir_parts = comparable_parts(&abs_d);
                    !dir_parts.is_empty() && path_parts.starts_with(&dir_parts)
                })
                .unwrap_or(false)
        }) || files.iter().any(|f| {
            self.resolve_in_root(f)
                .map(|abs_f| comparable_parts(&abs_f) == path_parts)
                .unwrap_or(false)
        })
    }
}

fn split_prefixed(entries: &[String], r_list: &mut Vec<String>, w_list: &mut Vec<String>) {
    for entry in entries {
        if let Some(rest) = entry.strip_prefix("w:") {
            w_list.push(rest.to_string());
            r_list.push(rest.to_string());
        } else if let Some(rest) = entry.strip_prefix("r:") {
            r_list.push(rest.to_string());
        }
    }
}

/// Python `os.path.abspath` analogue: join with process cwd, normalize `.` / `..`.
pub fn abspath(path: &str) -> PathBuf {
    let path = Path::new(path);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_lexical(&joined)
}

/// Resolve `path` against `root`: relative paths are joined with `root`,
/// absolute paths are kept, then `.` / `..` are normalized. The result must
/// stay inside `root` by path components (case-insensitive on Windows).
pub fn resolve_in_root(root: &Path, path: &str) -> Result<PathBuf, String> {
    let root = normalize_root(root);
    let candidate = Path::new(path);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let normalized = normalize_lexical(&joined);
    if path_starts_with(&normalized, &root) {
        Ok(normalized)
    } else {
        Err(format!(
            "path '{path}' is outside root cwd '{}'",
            root.display()
        ))
    }
}

/// Make `root` absolute (relative to process cwd) and normalize it lexically.
fn normalize_root(root: &Path) -> PathBuf {
    let joined = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(root)
    };
    normalize_lexical(&joined)
}

/// Normalize `.` and `..` lexically without touching the filesystem.
///
/// A `..` above the filesystem root is ignored, so the normalized path stays
/// absolute and can be checked safely with [`path_starts_with`].
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if out.file_name().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Component-prefix check; Windows paths are compared case-insensitively.
fn path_starts_with(path: &Path, root: &Path) -> bool {
    let path_parts = comparable_parts(path);
    let root_parts = comparable_parts(root);
    path_parts.starts_with(&root_parts)
}

fn comparable_parts(path: &Path) -> Vec<String> {
    path.components()
        .map(|c| {
            let s = c.as_os_str().to_string_lossy().replace('\\', "/");
            if cfg!(windows) {
                s.to_lowercase()
            } else {
                s
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root() -> PathBuf {
        std::env::current_dir().unwrap().join("test-root")
    }

    #[test]
    fn relative_path_inside_root_resolves() {
        let root = test_root();
        let sandbox = Sandbox::new(root.clone());
        assert_eq!(
            sandbox.resolve_in_root("sub/file.txt").unwrap(),
            root.join("sub").join("file.txt")
        );
    }

    #[test]
    fn parent_inside_root_is_normalized() {
        let root = test_root();
        let sandbox = Sandbox::new(root.clone());
        assert_eq!(
            sandbox.resolve_in_root("sub/../file.txt").unwrap(),
            root.join("file.txt")
        );
    }

    #[test]
    fn parent_outside_root_is_rejected() {
        let sandbox = Sandbox::new(test_root());
        assert!(sandbox.resolve_in_root("../outside.txt").is_err());
    }

    #[test]
    fn absolute_paths_are_checked_against_root() {
        let root = test_root();
        let sandbox = Sandbox::new(root.clone());

        let inside = root.join("inside.txt");
        assert_eq!(
            sandbox.resolve_in_root(&inside.to_string_lossy()).unwrap(),
            inside
        );

        let outside = root.parent().unwrap().join("outside.txt");
        assert!(sandbox.resolve_in_root(&outside.to_string_lossy()).is_err());
    }

    #[test]
    fn config_lists_are_relative_to_root() {
        let root = test_root();
        let available_dirs = vec!["w:src".to_string()];
        let deprecated_dirs = vec!["target".to_string()];
        let sandbox = Sandbox::from_fs(
            root.clone(),
            &available_dirs,
            &[],
            &[],
            &deprecated_dirs,
            &[],
        );

        assert!(sandbox.check_access("src/main.rs", 'w').is_ok());
        assert!(sandbox.check_access("target/main.rs", 'w').is_err());
    }
}
