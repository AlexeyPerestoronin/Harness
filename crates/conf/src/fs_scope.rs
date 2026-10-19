//! Lexical comparison of `tools.fs` allow-list and denylist paths.
//!
//! Paths are resolved against the process cwd without touching the filesystem.
//! `r:` / `w:` prefixes are stripped from allow-list entries. Separators `\` and
//! `/` are equal; a trailing separator is ignored because comparison is by path
//! components (`crates/conf` is not an ancestor of `crates/conf2`). On Windows
//! comparison is case-insensitive. A conflict is an allow-list path that
//! coincides with a denylist path or lies inside a deprecated directory.

use std::path::{Component, Path, PathBuf};

use crate::error::ConfError;
use crate::str::keys;

struct ResolvedPath {
    original: String,
    resolved: String,
    parts: Vec<String>,
}

/// Report every allow-list / denylist overlap in one error.
pub(crate) fn check_conflicts(
    available_dirs: &[String],
    available_files: &[String],
    deprecated_dirs: &[String],
    deprecated_files: &[String],
) -> Result<(), ConfError> {
    let a_dirs: Vec<_> = available_dirs
        .iter()
        .map(|e| resolve_available(e))
        .collect();
    let a_files: Vec<_> = available_files
        .iter()
        .map(|e| resolve_available(e))
        .collect();
    let d_dirs: Vec<_> = deprecated_dirs
        .iter()
        .map(|e| resolve_deprecated(e))
        .collect();
    let d_files: Vec<_> = deprecated_files
        .iter()
        .map(|e| resolve_deprecated(e))
        .collect();

    let mut lines = Vec::new();
    for a in &a_dirs {
        for d in &d_dirs {
            if let Some(reason) = overlap_reason(&a.parts, &d.parts) {
                lines.push(conflict_line(
                    keys::AVAILABLE_DIRS,
                    a,
                    reason,
                    keys::DEPRECATED_DIRS,
                    d,
                ));
            }
        }
    }
    for a in &a_files {
        for d in &d_files {
            if a.parts == d.parts {
                lines.push(conflict_line(
                    keys::AVAILABLE_FILES,
                    a,
                    "coincides with",
                    keys::DEPRECATED_FILES,
                    d,
                ));
            }
        }
    }
    for a in &a_files {
        for d in &d_dirs {
            if let Some(reason) = overlap_reason(&a.parts, &d.parts) {
                lines.push(conflict_line(
                    keys::AVAILABLE_FILES,
                    a,
                    reason,
                    keys::DEPRECATED_DIRS,
                    d,
                ));
            }
        }
    }

    if lines.is_empty() {
        return Ok(());
    }

    let mut msg = format!("{}.{}: conflicting settings\n", keys::TOOLS, keys::TOOL_FS);
    for line in &lines {
        msg.push_str("  - ");
        msg.push_str(line);
        msg.push('\n');
    }
    msg.push_str(&format!(
        "fix: remove the entry from one of the lists or narrow '{}'",
        keys::DEPRECATED_DIRS
    ));
    Err(ConfError::new(msg))
}

fn overlap_reason(available: &[String], deprecated: &[String]) -> Option<&'static str> {
    if available == deprecated {
        Some("coincides with")
    } else if available.len() > deprecated.len() && available.starts_with(deprecated) {
        Some("lies inside")
    } else {
        None
    }
}

fn conflict_line(
    left_key: &str,
    left: &ResolvedPath,
    reason: &str,
    right_key: &str,
    right: &ResolvedPath,
) -> String {
    format!(
        "'{left_key}' entry '{}' (resolved: {}) {reason} '{right_key}' entry '{}' (resolved: {})",
        left.original, left.resolved, right.original, right.resolved
    )
}

fn resolve_available(entry: &str) -> ResolvedPath {
    resolve_raw(entry, strip_rw_prefix(entry))
}

fn resolve_deprecated(entry: &str) -> ResolvedPath {
    resolve_raw(entry, entry)
}

fn resolve_raw(original: &str, path: &str) -> ResolvedPath {
    let abs = lexical_abspath(path);
    let resolved = abs.to_string_lossy().replace('\\', "/");
    let parts = comparable_parts(&abs);
    ResolvedPath {
        original: original.to_string(),
        resolved,
        parts,
    }
}

fn strip_rw_prefix(entry: &str) -> &str {
    entry
        .strip_prefix("r:")
        .or_else(|| entry.strip_prefix("w:"))
        .unwrap_or(entry)
}

/// Join with process cwd and normalize `.` / `..` without touching the filesystem.
/// `\` and `/` are treated as the same separator.
fn lexical_abspath(path: &str) -> PathBuf {
    let normalized = path.replace('\\', "/");
    let path = Path::new(&normalized);
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

fn comparable_parts(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Prefix(p) => Some(cmp_component(&p.as_os_str().to_string_lossy())),
            Component::RootDir => Some("/".to_string()),
            Component::Normal(n) => Some(cmp_component(&n.to_string_lossy())),
            Component::CurDir | Component::ParentDir => None,
        })
        .collect()
}

fn cmp_component(s: &str) -> String {
    let s = s.replace('\\', "/");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}
