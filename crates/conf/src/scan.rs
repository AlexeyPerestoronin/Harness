//! Scan a directory for agent json5 files without reading API keys.
//!
//! Only the directory root is listed (no recursion). A `.json5` file becomes a
//! candidate when vars-substitution + json5 parse yields an object with a known
//! `vendor` + `name` pair. `api-key`, `rpm` and session limits are not checked
//! here; full validation runs only for the file the caller chooses.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::ConfError;
use crate::str::keys;
use crate::types::{AgentCandidate, AgentKind};

/// A `.json5` file that was looked at and rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkippedFile {
    /// File name (not a path).
    pub file: String,
    /// Why it is not a candidate.
    pub reason: String,
}

/// Non-recursive scan of `dir` for agent config files.
pub(crate) fn scan_agent_dir(
    dir: &Path,
) -> Result<(Vec<AgentCandidate>, Vec<SkippedFile>), ConfError> {
    let mut files: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(dir)
        .map_err(|e| ConfError::new(format!("cannot read directory '{}': {e}", dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            ConfError::new(format!("cannot read directory '{}': {e}", dir.display()))
        })?;
        let path = entry.path();
        if path.is_file() && is_json5(&path) {
            files.push(path);
        }
    }
    files.sort_by(|a, b| {
        let na = file_name_lower(a);
        let nb = file_name_lower(b);
        na.cmp(&nb)
    });

    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for path in files {
        match candidate_from_file(&path) {
            Ok(c) => candidates.push(c),
            Err(reason) => skipped.push(SkippedFile {
                file: file_name(&path),
                reason,
            }),
        }
    }
    Ok((candidates, skipped))
}

fn is_json5(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json5"))
        .unwrap_or(false)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name_lower(path: &Path) -> String {
    file_name(path).to_ascii_lowercase()
}

/// Same read pipeline as a normal agent file (vars → json5), but only
/// `vendor` / `name` / `agent_kind` are inspected.
fn candidate_from_file(path: &Path) -> Result<AgentCandidate, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("cannot read: {e}"))?;
    let text =
        crate::vars::substitute(&text, &path.display().to_string()).map_err(|e| e.to_string())?;
    let root: Value = json5::from_str(&text).map_err(|e| format!("json5 parse error: {e}"))?;
    let obj = root
        .as_object()
        .ok_or_else(|| "not an object".to_string())?;
    let vendor = required_string(obj, keys::VENDOR)?;
    let name = required_string(obj, keys::NAME)?;
    AgentKind::from_vendor_name(&vendor, &name).map_err(|e| e.to_string())?;
    Ok(AgentCandidate {
        path: path.to_path_buf(),
        vendor,
        name,
    })
}

fn required_string(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    match obj.get(key) {
        None => Err(format!("missing '{key}'")),
        Some(v) => v
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| format!("'{key}' must be a string")),
    }
}
