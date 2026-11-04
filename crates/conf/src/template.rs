//! Textual JSON5 template for `harness.exe --conf-new`.
//!
//! The template text lives in `crates/conf/settings-template.json5` and is
//! embedded at compile time. Comments are part of that file (not via serde).

use std::fs;
use std::path::Path;

use crate::error::ConfError;

/// Write a full commented json5 template to `path`.
///
/// Fails if `path` already exists. Creates parent directories as needed.
pub fn write_new_conf(path: &Path) -> Result<(), ConfError> {
    if path.exists() {
        return Err(ConfError::new(format!(
            "conf file already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, template_text())?;
    Ok(())
}

/// Full JSON5 template with comments, defaults and `${vars:...}` placeholders.
pub fn template_text() -> String {
    include_str!("../settings-template.json5").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_has_no_cwd_and_has_deprecated_dir_patterns() {
        let text = template_text();
        assert!(!text.contains("\"cwd\""));
        assert!(!text.contains("workspace"));
        assert!(text.contains("deprecated-dir-patterns"));
        assert!(text.contains("glob pattern"));
    }
}
