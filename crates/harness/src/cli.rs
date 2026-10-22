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
