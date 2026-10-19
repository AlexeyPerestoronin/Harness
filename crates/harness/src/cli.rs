//! Command-line interface for `harness.exe`.
//!
//! Parsing is explicit (no clap) so `--help` is the compile-time `help.txt`
//! and the xor rules for `--prompt` / `--dump` / `--conf-new` are checked in one
//! place. `--conf` defaults to `<process-cwd>/harness.json5`.

use std::path::PathBuf;

/// Help text embedded at compile time from `harness/crates/harness/help.txt`.
pub const HELP: &str = include_str!("../help.txt");

/// Action selected by the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    /// Print [`HELP`] and exit 0.
    Help,
    /// Write a conf template and exit.
    ConfNew {
        /// Destination path for the new json5 file.
        path: PathBuf,
    },
    /// Start or resume an agent session.
    Start {
        /// Path to `harness.json5`.
        conf: PathBuf,
        /// New-session prompt. `None` when resuming from dump.
        prompt: Option<String>,
        /// Dump file to resume from.
        dump: Option<PathBuf>,
    },
}

/// Parse argv **without** `argv[0]`.
pub fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut conf: Option<PathBuf> = None;
    let mut prompt: Option<String> = None;
    let mut dump: Option<PathBuf> = None;
    let mut conf_new: Option<PathBuf> = None;
    let mut help = false;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--help" || a == "-h" {
            help = true;
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

    let n = [prompt.is_some(), dump.is_some(), conf_new.is_some()]
        .iter()
        .filter(|x| **x)
        .count();
    if n == 0 {
        return Err("нужен --prompt, --dump или --conf-new".to_string());
    }
    if n > 1 {
        return Err("--prompt, --dump и --conf-new взаимоисключающие".to_string());
    }
    if conf_new.is_some() && conf.is_some() {
        return Err("--conf нельзя использовать вместе с --conf-new".to_string());
    }

    if let Some(path) = conf_new {
        return Ok(CliAction::ConfNew { path });
    }

    let conf = conf.unwrap_or_else(default_conf_path);
    Ok(CliAction::Start { conf, prompt, dump })
}

/// Default conf path: `<process-cwd>/harness.json5`.
pub fn default_conf_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("harness.json5")
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

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn help_contains_conf_new() {
        assert!(HELP.contains("--conf-new"));
        assert!(HELP.contains("--prompt"));
        assert!(HELP.contains("--dump"));
    }

    #[test]
    fn help_flag() {
        assert_eq!(parse_args(&s(&["--help"])).unwrap(), CliAction::Help);
        assert_eq!(parse_args(&s(&["-h"])).unwrap(), CliAction::Help);
        assert_eq!(
            parse_args(&s(&["--help", "--prompt", "x"])).unwrap(),
            CliAction::Help
        );
    }

    #[test]
    fn missing_action_is_error() {
        let err = parse_args(&s(&[])).unwrap_err();
        assert!(err.contains("--prompt"), "{err}");
        let err = parse_args(&s(&["--conf", "a.json5"])).unwrap_err();
        assert!(err.contains("--prompt"), "{err}");
    }

    #[test]
    fn xor_prompt_dump_conf_new() {
        assert!(parse_args(&s(&["--prompt", "a", "--dump", "b"])).is_err());
        assert!(parse_args(&s(&["--prompt", "a", "--conf-new", "c"])).is_err());
        assert!(parse_args(&s(&["--dump", "b", "--conf-new", "c"])).is_err());
        assert!(parse_args(&s(&["--prompt", "a", "--dump", "b", "--conf-new", "c"])).is_err());
    }

    #[test]
    fn conf_with_conf_new_is_error() {
        let err = parse_args(&s(&["--conf", "a.json5", "--conf-new", "b.json5"])).unwrap_err();
        assert!(err.contains("--conf"), "{err}");
    }

    #[test]
    fn prompt_uses_default_conf() {
        match parse_args(&s(&["--prompt", "hello"])).unwrap() {
            CliAction::Start { conf, prompt, dump } => {
                assert_eq!(conf, default_conf_path());
                assert_eq!(prompt.as_deref(), Some("hello"));
                assert!(dump.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dump_and_custom_conf() {
        match parse_args(&s(&["--conf", "x.json5", "--dump", "d.json"])).unwrap() {
            CliAction::Start { conf, prompt, dump } => {
                assert_eq!(conf, PathBuf::from("x.json5"));
                assert!(prompt.is_none());
                assert_eq!(dump, Some(PathBuf::from("d.json")));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn conf_new() {
        match parse_args(&s(&["--conf-new", "out.json5"])).unwrap() {
            CliAction::ConfNew { path } => assert_eq!(path, PathBuf::from("out.json5")),
            other => panic!("{other:?}"),
        }
    }
}
