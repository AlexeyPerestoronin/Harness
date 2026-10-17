//! Native `harness.exe` entry point.
//!
//! Wires CLI → conf crate → agent loop. Configuration files are opened only
//! inside `conf`; this binary receives typed settings.

mod agent_loop;
mod agents;
mod cli;
#[cfg(test)]
mod agents_live_tests;

use std::path::PathBuf;
use std::process::ExitCode;

use cli::{parse_args, CliAction, HELP};

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
        CliAction::Help => {
            print!("{HELP}");
            if !HELP.ends_with('\n') {
                println!();
            }
            Ok(())
        }
        CliAction::ConfNew { path } => {
            conf::write_new_conf(&path).map_err(|e| e.to_string())?;
            Ok(())
        }
        CliAction::Start { conf, prompt, dump } => start_session(conf, prompt, dump),
    }
}

fn start_session(
    conf_path: PathBuf,
    prompt: Option<String>,
    dump: Option<PathBuf>,
) -> Result<(), String> {
    let settings = conf::load(&conf_path).map_err(|e| e.to_string())?;
    let log_dir = agent_loop::session_dir(&settings)?;
    let log_file = log_dir.join("log.md");
    let dump_file = log_dir.join("dump.json");
    let mut loop_ = agent_loop::AgentLoop::new(&settings, &log_file, log_dir)?;
    loop_.start(prompt, &dump_file, dump.as_deref())
}
