//! Native `harness.exe` entry point.
//!
//! Wires CLI → conf crate → agent loop. Configuration files are opened only
//! inside `conf`; this binary receives typed settings.

mod agent_loop;
mod agent_select;
mod agents;
mod cli;
mod dump_settings;
mod vendor_request;

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use cli::{parse_args, read_prompt_file, version_line, CliAction, HELP};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args)? {
        CliAction::Help => print_help(HELP),
        CliAction::HelpTools => print_help(tools::HELP_USER),
        CliAction::Version => {
            println!("{}", version_line());
            Ok(())
        }
        CliAction::ConfNew { path } => {
            conf::write_new_conf(&path).map_err(|e| e.to_string())?;
            Ok(())
        }
        CliAction::Start {
            conf,
            prompt,
            prompt_file,
            dump,
            cwd,
            api_key,
        } => {
            let prompt = match prompt_file {
                Some(path) => Some(read_prompt_file(&path)?),
                None => prompt,
            };
            start_session(conf, prompt, dump, cwd, api_key)
        }
    }
}

fn start_session(
    conf_path: Option<PathBuf>,
    prompt: Option<String>,
    dump: Option<PathBuf>,
    cwd: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<(), String> {
    let (mut settings, source_dump) = if let Some(dump_path) = dump {
        let settings_value = dump_settings::extract(&dump_path)?;
        let conf_dir = dump_settings::conf_dir_of(&settings_value)?;
        let settings = conf::load_from_value(settings_value, &conf_dir, cwd, agent_select::choose)
            .map_err(|e| e.to_string())?;
        (settings, Some(dump_path))
    } else {
        let conf_path = conf_path.ok_or_else(|| "missing --conf".to_string())?;
        let settings =
            conf::load_with(&conf_path, cwd, agent_select::choose).map_err(|e| e.to_string())?;
        (settings, None)
    };

    if let Some(raw) = api_key {
        apply_api_key_override(&mut settings, &raw)?;
    }
    if settings.agent.api_key.is_empty() {
        settings.agent.api_key = prompt_api_key(&settings.agent.vendor, &settings.agent.name)?;
    }

    let log_dir = agent_loop::session_dir(&settings)?;
    let log_file = log_dir.join("log.md");
    let dump_file = log_dir.join("dump.json");
    let mut loop_ = agent_loop::AgentLoop::new(&settings, &log_file, log_dir)?;
    loop_.start(prompt, &dump_file, source_dump.as_deref())
}

fn apply_api_key_override(settings: &mut conf::Settings, raw: &str) -> Result<(), String> {
    let candidate = PathBuf::from(raw);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        settings.cwd.join(candidate)
    };
    if path.is_file() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read api-key file '{}': {e}", path.display()))?;
        settings.agent.api_key = contents.trim().to_string();
    } else {
        settings.agent.api_key = raw.to_string();
    }
    Ok(())
}

fn prompt_api_key(vendor: &str, name: &str) -> Result<String, String> {
    print!("Enter API key for {vendor} {name}: ");
    io::stdout()
        .flush()
        .map_err(|e| format!("cannot prompt for api key: {e}"))?;
    let mut buf = String::new();
    let read = io::stdin()
        .read_line(&mut buf)
        .map_err(|e| format!("cannot read api key: {e}"))?;
    if read == 0 {
        return Err("api-key is required".to_string());
    }
    let key = buf.trim();
    if key.is_empty() {
        return Err("api-key is required".to_string());
    }
    Ok(key.to_string())
}

/// Print embedded help and guarantee a trailing newline.
fn print_help(text: &str) -> Result<(), String> {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}
