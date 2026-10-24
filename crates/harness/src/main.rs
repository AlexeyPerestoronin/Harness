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
        } => {
            let prompt = match prompt_file {
                Some(path) => Some(read_prompt_file(&path)?),
                None => prompt,
            };
            start_session(conf, prompt, dump)
        }
    }
}

fn start_session(
    conf_path: Option<PathBuf>,
    prompt: Option<String>,
    dump: Option<PathBuf>,
) -> Result<(), String> {
    let (settings, source_dump) = if let Some(dump_path) = dump {
        let settings_value = dump_settings::extract(&dump_path)?;
        let conf_dir = dump_settings::conf_dir_of(&settings_value)?;
        let settings = conf::load_from_value(settings_value, &conf_dir, agent_select::choose)
            .map_err(|e| e.to_string())?;
        (settings, Some(dump_path))
    } else {
        let conf_path = conf_path.ok_or_else(|| "missing --conf".to_string())?;
        let settings =
            conf::load_with(&conf_path, agent_select::choose).map_err(|e| e.to_string())?;
        (settings, None)
    };
    let log_dir = agent_loop::session_dir(&settings)?;
    let log_file = log_dir.join("log.md");
    let dump_file = log_dir.join("dump.json");
    let mut loop_ = agent_loop::AgentLoop::new(&settings, &log_file, log_dir)?;
    loop_.start(prompt, &dump_file, source_dump.as_deref())
}

/// Print embedded help and guarantee a trailing newline.
fn print_help(text: &str) -> Result<(), String> {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}
