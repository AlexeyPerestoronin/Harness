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
        available_dirs: &[String],
        available_files: &[String],
        extensions: &[String],
        deprecated_dirs: &[String],
        deprecated_files: &[String],
    ) -> Self {
        let mut sb = Self {
            extensions: extensions.to_vec(),
            deprecated_dirs: deprecated_dirs.to_vec(),
            deprecated_files: deprecated_files.to_vec(),
            ..Self::default()
        };
        split_prefixed(available_dirs, &mut sb.r_dirs, &mut sb.w_dirs);
        split_prefixed(available_files, &mut sb.r_files, &mut sb.w_files);
        sb
    }

    /// Empty allow-list (no `fs` module in conf).
    pub fn empty() -> Self {
        Self::default()
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
        if self.is_deprecated(path) {
            return Err(format!(
                "{mode}-access denied to {path} → path is deprecated (excluded by 'deprecated-dirs'/'deprecated-files' in tools.fs; do not use it)"
            ));
        }
        let (dirs, files) = if mode == 'w' {
            (&self.w_dirs, &self.w_files)
        } else {
            (&self.r_dirs, &self.r_files)
        };
        if self.is_allowed(path, dirs, files) {
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
        if self.is_deprecated(path) {
            return Err(format!(
                "{mode}-access denied to '{path}': path is deprecated"
            ));
        }
        let is_dir = Path::new(path).is_dir();
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
        let path_parts = comparable_parts(&abspath(path));
        if self
            .deprecated_files
            .iter()
            .any(|f| comparable_parts(&abspath(f)) == path_parts)
        {
            return true;
        }
        self.deprecated_dirs.iter().any(|d| {
            let dir_parts = comparable_parts(&abspath(d));
            !dir_parts.is_empty() && path_parts.starts_with(&dir_parts)
        })
    }

    fn is_allowed(&self, path: &str, dirs: &[String], files: &[String]) -> bool {
        let abs_p = abspath(path);
        let abs_p_s = path_to_string(&abs_p);
        dirs.iter()
            .any(|d| abs_p_s.starts_with(&path_to_string(&abspath(d))))
            || files.iter().any(|f| abs_p_s == path_to_string(&abspath(f)))
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
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sandbox-{label}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn prefix_dir_and_exact_file() {
        let root = tmp("pfx");
        let allowed = root.join("allowed");
        fs::create_dir_all(&allowed).unwrap();
        let inside = allowed.join("a.txt");
        fs::write(&inside, "x").unwrap();
        let outside = root.join("secret.txt");
        fs::write(&outside, "y").unwrap();
        let extra = root.join("one.txt");
        fs::write(&extra, "z").unwrap();

        let sb = Sandbox::from_fs(
            &[format!("w:{}", allowed.display())],
            &[format!("r:{}", extra.display())],
            &[],
            &[],
            &[],
        );
        assert!(sb.check_access(&inside.to_string_lossy(), 'r').is_ok());
        assert!(sb.check_access(&inside.to_string_lossy(), 'w').is_ok());
        assert!(sb.check_access(&outside.to_string_lossy(), 'r').is_err());
        assert!(sb.check_access(&extra.to_string_lossy(), 'r').is_ok());
        assert!(sb.check_access(&extra.to_string_lossy(), 'w').is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_implies_read() {
        let sb = Sandbox::from_fs(&["w:src".into()], &[], &[], &[], &[]);
        assert_eq!(sb.w_dirs(), &["src".to_string()]);
        assert_eq!(sb.r_dirs(), &["src".to_string()]);
    }

    #[test]
    fn empty_extensions_allow_any() {
        let sb = Sandbox::from_fs(&["w:.".into()], &[], &[], &[], &[]);
        assert!(sb.check_extensions("foo.bin", 'r').is_ok());
    }

    #[test]
    fn extensions_filter() {
        let sb = Sandbox::from_fs(&["w:.".into()], &[], &[".md".into()], &[], &[]);
        assert!(sb.check_extensions("notes.md", 'r').is_ok());
        assert!(sb.check_extensions("notes.py", 'r').is_err());
    }

    #[test]
    fn grant_dir_and_file() {
        let root = tmp("grant");
        let dir = root.join("d");
        fs::create_dir_all(&dir).unwrap();
        let file = root.join("f.txt");
        fs::write(&file, "x").unwrap();
        let mut sb = Sandbox::empty();
        sb.grant_path_access(&dir.to_string_lossy(), 'w').unwrap();
        sb.grant_path_access(&file.to_string_lossy(), 'r').unwrap();
        assert!(sb.w_dirs().iter().any(|p| p == &dir.to_string_lossy()));
        assert!(sb.r_dirs().iter().any(|p| p == &dir.to_string_lossy()));
        assert!(sb.r_files().iter().any(|p| p == &file.to_string_lossy()));
        assert!(sb.w_files().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    fn layout() -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf, Sandbox) {
        let root = tmp("dep");
        let target = root.join("target");
        let target2 = root.join("target2");
        let mytarget = root.join("mytarget");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&target2).unwrap();
        fs::create_dir_all(&mytarget).unwrap();
        let inside = target.join("x.txt");
        fs::write(&inside, "x").unwrap();
        fs::write(target2.join("x.txt"), "x").unwrap();
        fs::write(mytarget.join("x.txt"), "x").unwrap();
        let banned = root.join("secret.log");
        fs::write(&banned, "no").unwrap();
        let sb = Sandbox::from_fs(
            &[format!("w:{}", root.display())],
            &[],
            &[],
            &[target.display().to_string()],
            &[banned.display().to_string()],
        );
        (root, target, target2, mytarget, banned, sb)
    }

    #[test]
    fn check_access_denies_deprecated_dir_file_and_exact_file() {
        let (root, target, target2, mytarget, banned, sb) = layout();
        let inside = target.join("x.txt");
        for mode in ['r', 'w'] {
            let err = sb
                .check_access(&target.to_string_lossy(), mode)
                .unwrap_err();
            assert!(err.contains("deprecated"), "{err}");
            assert!(!err.contains("secret.log"), "{err}");
            let err = sb
                .check_access(&inside.to_string_lossy(), mode)
                .unwrap_err();
            assert!(err.contains("deprecated"), "{err}");
            let err = sb
                .check_access(&banned.to_string_lossy(), mode)
                .unwrap_err();
            assert!(err.contains("deprecated"), "{err}");
            assert!(sb
                .check_access(&target2.join("x.txt").to_string_lossy(), mode)
                .is_ok());
            assert!(sb
                .check_access(&mytarget.join("x.txt").to_string_lossy(), mode)
                .is_ok());
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn grant_path_access_refuses_deprecated() {
        let (root, target, _, _, banned, mut sb) = layout();
        let err = sb
            .grant_path_access(&target.to_string_lossy(), 'r')
            .unwrap_err();
        assert!(err.contains("deprecated"), "{err}");
        assert!(!sb.r_dirs().iter().any(|p| p == &target.to_string_lossy()));
        let err = sb
            .grant_path_access(&banned.to_string_lossy(), 'w')
            .unwrap_err();
        assert!(err.contains("deprecated"), "{err}");
        assert!(sb.r_files().is_empty());
        assert!(sb.w_files().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn deprecated_match_is_case_insensitive_on_windows() {
        let root = tmp("case");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        let inside = target.join("x.txt");
        fs::write(&inside, "x").unwrap();
        let sb = Sandbox::from_fs(
            &[format!("w:{}", root.display())],
            &[],
            &[],
            &[target.display().to_string()],
            &[],
        );
        let mixed = root.join("TARGET").join("x.txt");
        let err = sb.check_access(&mixed.to_string_lossy(), 'r').unwrap_err();
        assert!(err.contains("deprecated"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }
}
