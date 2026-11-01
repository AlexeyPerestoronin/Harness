//! Command-line interface for `harness.exe`.
//!
//! Parsing is explicit (no clap) so `--help` is the compile-time `help.txt`,
//! `--help-tools` is `tools::HELP_USER`, `--version` is `CARGO_PKG_VERSION`,
//! and the xor rules for `--prompt` / `--prompt-file` / `--dump` / `--conf-new`
//! are checked in one place. `--conf` defaults to `<process-cwd>/harness.json5`
//! for a new session and cannot be combined with `--dump`.

use std::path::{Path, PathBuf};

/// Help text embedded at compile time from `crates/cli-harness/help.txt`.
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
    /// Start the HTTPS server.
    Serve {
        /// Path to `harness.json5`. Defaults to `<cwd>/harness.json5`.
        conf: PathBuf,
        /// Workspace override. Resolved against the working conf file directory.
        cwd: Option<PathBuf>,
        /// CLI port override.
        port: Option<u16>,
        /// CLI bind host override.
        host: Option<String>,
        /// CLI TLS certificate override.
        tls_cert: Option<PathBuf>,
        /// CLI TLS private key override.
        tls_key: Option<PathBuf>,
        /// Fixed bearer token override.
        auth_token: Option<String>,
    },
    /// Start or resume an agent session.
    Start {
        /// Path to `harness.json5`. `None` when resuming from dump.
        conf: Option<PathBuf>,
        /// New-session prompt. `None` when resuming from dump or when the
        /// prompt is supplied via [`Self::Start::prompt_file`].
        prompt: Option<String>,
        /// Path to a UTF-8 file whose contents become the new-session prompt.
        /// Read after parsing, not inside [`parse_args`].
        prompt_file: Option<PathBuf>,
        /// Dump file to resume from.
        dump: Option<PathBuf>,
        /// Workspace override. Resolved against the working conf file directory.
        cwd: Option<PathBuf>,
        /// API-key override: a literal key or a path to a UTF-8 key file.
        api_key: Option<String>,
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
    let mut cwd: Option<PathBuf> = None;
    let mut api_key: Option<String> = None;
    let mut serve = false;
    let mut port: Option<u16> = None;
    let mut host: Option<String> = None;
    let mut tls_cert: Option<PathBuf> = None;
    let mut tls_key: Option<PathBuf> = None;
    let mut auth_token: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--serve" {
            serve = true;
            i += 1;
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--port")? {
            if value.trim().is_empty() {
                return Err("--port requires a port number in range 1..=65535".to_string());
            }
            let parsed = value
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|p| *p >= 1)
                .ok_or_else(|| "--port requires a port number in range 1..=65535".to_string())?;
            port = Some(parsed);
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--host")? {
            if value.trim().is_empty() {
                return Err("--host requires a non-empty value".to_string());
            }
            host = Some(value);
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--tls-cert")? {
            if value.trim().is_empty() {
                return Err("--tls-cert requires a non-empty path".to_string());
            }
            tls_cert = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--tls-key")? {
            if value.trim().is_empty() {
                return Err("--tls-key requires a non-empty path".to_string());
            }
            tls_key = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--auth-token")? {
            if value.trim().is_empty() {
                return Err("--auth-token requires a non-empty token".to_string());
            }
            auth_token = Some(value);
            continue;
        }
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
        if let Some(value) = take_value(args, &mut i, "--cwd")? {
            if value.is_empty() {
                return Err("--cwd requires a value".to_string());
            }
            cwd = Some(PathBuf::from(value));
            continue;
        }
        if let Some(value) = take_value(args, &mut i, "--api-key")? {
            if value.is_empty() {
                return Err("--api-key requires a non-empty value".to_string());
            }
            api_key = Some(value);
            continue;
        }
        return Err(format!("unknown argument: {a}"));
    }

    if args.is_empty() {
        serve = true;
    }

    let server_flag_used = port.is_some()
        || host.is_some()
        || tls_cert.is_some()
        || tls_key.is_some()
        || auth_token.is_some();
    if server_flag_used && !serve {
        return Err(
            "--port, --host, --tls-cert, --tls-key and --auth-token can only be used with --serve"
                .to_string(),
        );
    }
    if serve
        && (prompt.is_some()
            || prompt_file.is_some()
            || dump.is_some()
            || conf_new.is_some()
            || api_key.is_some())
    {
        return Err(
            "--serve cannot be combined with --prompt, --prompt-file, --dump, --conf-new or --api-key"
                .to_string(),
        );
    }
    if tls_cert.is_some() != tls_key.is_some() {
        return Err("--tls-cert and --tls-key must be used together".to_string());
    }

    if cwd.is_some() || api_key.is_some() {
        if help || help_tools || version || conf_new.is_some() {
            return Err(
                "--cwd and --api-key can only be used with --prompt, --prompt-file or --dump"
                    .to_string(),
            );
        }
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
    if serve {
        let conf = if let Some(conf) = conf {
            conf
        } else {
            default_conf_path_for_serve(cwd.as_deref())
        };
        return Ok(CliAction::Serve {
            conf,
            cwd,
            port,
            host,
            tls_cert,
            tls_key,
            auth_token,
        });
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
    if dump.is_some() && conf.is_some() {
        return Err(
            "--conf cannot be used with --dump (configuration is taken from the dump file)"
                .to_string(),
        );
    }

    if let Some(path) = conf_new {
        return Ok(CliAction::ConfNew { path });
    }

    let conf = if dump.is_some() {
        None
    } else {
        Some(conf.unwrap_or_else(default_conf_path))
    };
    Ok(CliAction::Start {
        conf,
        prompt,
        prompt_file,
        dump,
        cwd,
        api_key,
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

/// Default conf path for the server mode: `<cwd>/harness.json5`.
///
/// When `--cwd` is not supplied the process current directory is used.
pub fn default_conf_path_for_serve(cwd: Option<&Path>) -> PathBuf {
    let base = cwd
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    base.join("harness.json5")
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

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_starts_server_with_default_conf() {
        let action = parse_args(&[]).unwrap();
        match action {
            CliAction::Serve {
                conf,
                cwd,
                port,
                host,
                tls_cert,
                tls_key,
                auth_token,
            } => {
                assert_eq!(conf, default_conf_path_for_serve(None));
                assert!(cwd.is_none());
                assert!(port.is_none());
                assert!(host.is_none());
                assert!(tls_cert.is_none());
                assert!(tls_key.is_none());
                assert!(auth_token.is_none());
            }
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    #[test]
    fn serve_parses_all_server_flags() {
        let action = parse_args(&args(&[
            "--serve",
            "--port",
            "4090",
            "--host",
            "0.0.0.0",
            "--tls-cert",
            "cert.pem",
            "--tls-key",
            "key.pem",
            "--auth-token",
            "token",
        ]))
        .unwrap();
        match action {
            CliAction::Serve {
                conf,
                cwd: _,
                port,
                host,
                tls_cert,
                tls_key,
                auth_token,
            } => {
                assert_eq!(conf, default_conf_path_for_serve(None));
                assert_eq!(port, Some(4090));
                assert_eq!(host.as_deref(), Some("0.0.0.0"));
                assert_eq!(tls_cert, Some(PathBuf::from("cert.pem")));
                assert_eq!(tls_key, Some(PathBuf::from("key.pem")));
                assert_eq!(auth_token.as_deref(), Some("token"));
            }
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    #[test]
    fn port_without_serve_is_rejected() {
        let err = parse_args(&args(&["--port", "4067"])).unwrap_err();
        assert!(err.contains("--serve"), "{err}");
    }

    #[test]
    fn tls_flags_require_both() {
        let err = parse_args(&args(&["--serve", "--tls-cert", "cert.pem"])).unwrap_err();
        assert!(err.contains("must be used together"), "{err}");
    }

    #[test]
    fn serve_conflicts_with_console_modes() {
        let err = parse_args(&args(&["--serve", "--prompt", "x"])).unwrap_err();
        assert!(err.contains("--serve"), "{err}");
    }
}
