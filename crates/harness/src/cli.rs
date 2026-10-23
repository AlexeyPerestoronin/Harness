//! Command-line interface for `harness.exe`.
//!
//! Parsing is explicit (no clap) so `--help` is the compile-time `help.txt`,
//! `--help-tools` is `tools::HELP_USER`, `--version` is `CARGO_PKG_VERSION`,
//! and the xor rules for `--prompt` / `--prompt-file` / `--dump` / `--conf-new`
//! are checked in one place. `--conf` defaults to `<process-cwd>/harness.json5`.

use std::path::{Path, PathBuf};

/// Help text embedded at compile time from `harness/crates/harness/help.txt`.
pub const HELP: &str = include_str!("../help.txt");

/// Action selected by the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    /// Print [`HELP`] and exit 0.
    Help,
    /// Print [`tools::HELP_USER`] and exit 0.
    HelpTools,
    /// Print the application version and exit 0.
    Version,
    /// Write a conf template and exit.
    ConfNew {
        /// Destination path for the new json5 file.
        path: PathBuf,
    },
    /// Start or resume an agent session.
    Start {
        /// Path to `harness.json5`.
        conf: PathBuf,
        /// New-session prompt. `None` when resuming from dump or when the
        /// prompt is supplied via [`Self::Start::prompt_file`].
        prompt: Option<String>,
        /// Path to a UTF-8 file whose contents become the new-session prompt.
        /// Read after parsing, not inside [`parse_args`].
        prompt_file: Option<PathBuf>,
        /// Dump file to resume from.
        dump: Option<PathBuf>,
    },
}

/// Parse argv **without** `argv[0]`.
pub fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut conf: Option<PathBuf> = None;
    let mut prompt: Option<String> = None;
    let mut prompt_file: Option<PathBuf> = None;
    let mut dump: Option<PathBuf> = None;
    let mut conf_new: Option<PathBuf> = None;
    let mut help = false;
    let mut help_tools = false;
    let mut version = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--help" || a == "-h" {
            help = true;
            i += 1;
            continue;
        }
        if a == "--help-tools" {
            help_tools = true;
            i += 1;
            continue;
        }
        if a == "--version" {
            version = true;
            i += 1;
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--conf")? {
            conf = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--prompt")? {
            prompt = Some(value);
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--prompt-file")? {
            prompt_file = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--dump")? {
            dump = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--conf-new")? {
            conf_new = Some(PathBuf::from(value));
            continue;
        }
        return Err(format!("unknown argument: {a}"));
    }

    if help {
        return Ok(CliAction::Help);
    }
    if help_tools {
        return Ok(CliAction::HelpTools);
    }
    if version {
        return Ok(CliAction::Version);
    }

    let n = [
        prompt.is_some(),
        prompt_file.is_some(),
        dump.is_some(),
        conf_new.is_some(),
    ]
    .iter()
    .filter(|x| **x)
    .count();
    if n == 0 {
        return Err("нужен --prompt, --prompt-file, --dump или --conf-new".to_string());
    }
    if n > 1 {
        return Err("--prompt, --prompt-file, --dump и --conf-new взаимоисключающие".to_string());
    }
    if conf_new.is_some() && conf.is_some() {
        return Err("--conf нельзя использовать вместе с --conf-new".to_string());
    }

    if let Some(path) = conf_new {
        return Ok(CliAction::ConfNew { path });
    }

    let conf = conf.unwrap_or_else(default_conf_path);
    Ok(CliAction::Start {
        conf,
        prompt,
        prompt_file,
        dump,
    })
}

/// Read a `--prompt-file` path as UTF-8.
///
/// Relative paths are resolved from the process current directory by the OS.
/// A single leading BOM (`\u{feff}`) is stripped. Surrounding whitespace is kept.
pub fn read_prompt_file(path: &Path) -> Result<String, String> {
    if path.is_dir() {
        return Err(format!(
            "cannot read prompt file '{}': is a directory",
            path.display()
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read prompt file '{}': {e}", path.display()))?;
    let mut text = String::from_utf8(bytes).map_err(|_| {
        format!(
            "cannot read prompt file '{}': invalid utf-8",
            path.display()
        )
    })?;
    if let Some(rest) = text.strip_prefix('\u{feff}') {
        text = rest.to_string();
    }
    if text.is_empty() {
        return Err(format!("prompt file '{}' is empty", path.display()));
    }
    Ok(text)
}

/// Default conf path: `<process-cwd>/harness.json5`.
pub fn default_conf_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("harness.json5")
}

/// One-line version string for `--version`: `harness <X.Y.Z>`.
pub fn version_line() -> String {
    format!("harness {}", env!("CARGO_PKG_VERSION"))
}

fn take_value(args: &[String], i: &mut usize, flag: &str) -> Result<Option<String>, String> {
    let a = args[*i].as_str();
    let prefix = format!("{flag}=");
    if a == flag {
        let value = args
            .get(*i + 1)
            .cloned()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        *i += 2;
        return Ok(Some(value));
    }
    if let Some(rest) = a.strip_prefix(&prefix) {
        *i += 1;
        return Ok(Some(rest.to_string()));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    fn temp_prompt_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "harness-cli-prompt-file-{label}-{}.txt",
            std::process::id()
        ))
    }

    #[test]
    fn prompt_file_space_separated() {
        assert_eq!(
            parse_args(&args(&["--prompt-file", "p.txt"])).unwrap(),
            CliAction::Start {
                conf: default_conf_path(),
                prompt: None,
                prompt_file: Some(PathBuf::from("p.txt")),
                dump: None,
            }
        );
    }

    #[test]
    fn prompt_file_equals() {
        assert_eq!(
            parse_args(&args(&["--prompt-file=p.txt"])).unwrap(),
            CliAction::Start {
                conf: default_conf_path(),
                prompt: None,
                prompt_file: Some(PathBuf::from("p.txt")),
                dump: None,
            }
        );
    }

    #[test]
    fn prompt_and_prompt_file_are_xor() {
        let err = parse_args(&args(&["--prompt", "x", "--prompt-file", "p"])).unwrap_err();
        assert_eq!(
            err,
            "--prompt, --prompt-file, --dump и --conf-new взаимоисключающие"
        );
    }

    #[test]
    fn prompt_file_and_dump_are_xor() {
        let err = parse_args(&args(&["--prompt-file", "p", "--dump", "d"])).unwrap_err();
        assert_eq!(
            err,
            "--prompt, --prompt-file, --dump и --conf-new взаимоисключающие"
        );
    }

    #[test]
    fn missing_xor_flag_is_error() {
        let err = parse_args(&args(&[])).unwrap_err();
        assert_eq!(err, "нужен --prompt, --prompt-file, --dump или --conf-new");
    }

    #[test]
    fn help_wins_over_prompt_file() {
        assert_eq!(
            parse_args(&args(&["--help", "--prompt-file", "p"])).unwrap(),
            CliAction::Help
        );
    }

    #[test]
    fn help_text_mentions_prompt_file() {
        assert!(HELP.contains("--prompt-file"));
    }

    #[test]
    fn read_prompt_file_keeps_surrounding_whitespace() {
        let path = temp_prompt_path("ws");
        std::fs::write(&path, "  hello \n").unwrap();
        let result = read_prompt_file(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.unwrap(), "  hello \n");
    }

    #[test]
    fn read_prompt_file_empty_is_error() {
        let path = temp_prompt_path("empty");
        std::fs::write(&path, b"").unwrap();
        let result = read_prompt_file(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            result.unwrap_err(),
            format!("prompt file '{}' is empty", path.display())
        );
    }

    #[test]
    fn read_prompt_file_directory_is_error() {
        let path = std::env::temp_dir();
        assert_eq!(
            read_prompt_file(&path).unwrap_err(),
            format!(
                "cannot read prompt file '{}': is a directory",
                path.display()
            )
        );
    }

    #[test]
    fn read_prompt_file_strips_one_leading_bom() {
        let path = temp_prompt_path("bom");
        std::fs::write(&path, "\u{feff}  hi \n").unwrap();
        let result = read_prompt_file(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(result.unwrap(), "  hi \n");
    }

    #[test]
    fn read_prompt_file_bom_only_is_empty() {
        let path = temp_prompt_path("bom-only");
        std::fs::write(&path, "\u{feff}").unwrap();
        let result = read_prompt_file(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            result.unwrap_err(),
            format!("prompt file '{}' is empty", path.display())
        );
    }

    #[test]
    fn read_prompt_file_invalid_utf8_is_error() {
        let path = temp_prompt_path("bin");
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        let result = read_prompt_file(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            result.unwrap_err(),
            format!(
                "cannot read prompt file '{}': invalid utf-8",
                path.display()
            )
        );
    }
}
