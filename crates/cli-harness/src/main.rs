//! Native `harness.exe` entry point.
//!
//! Wires CLI → conf crate → asynchronous agent loop. Configuration files are
//! opened only inside `conf`; this binary receives typed settings.

mod cli;
mod server;
mod session;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use agent_loop::RunState;
use cli::{parse_args, read_prompt_file, version_line, CliAction, HELP};
use conf::ServerConfigState;
use interactions::{CliInteraction, Interaction, InteractionLoggers};
use logger::{FileLogger, Logger};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<(), String> {
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
        CliAction::Serve {
            conf,
            cwd,
            port,
            host,
            tls_cert,
            tls_key,
            auth_token,
        } => {
            server::serve(server::ServeOptions {
                conf,
                cwd,
                port,
                host,
                tls_cert,
                tls_key,
                auth_token,
            })
            .await
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
            match (dump, conf) {
                (Some(dump_path), _) => start_resume_session(dump_path, api_key).await,
                (None, Some(conf_path)) => start_new_session(conf_path, prompt, cwd, api_key).await,
                (None, None) => Err("missing --conf".to_string()),
            }
        }
    }
}

async fn start_new_session(
    conf_path: PathBuf,
    prompt: Option<String>,
    cwd_override: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<(), String> {
    let cwd = session::resolve_cwd(cwd_override.as_deref())?;
    let conf_path = session::resolve_abs_path(&conf_path)?;
    println!("cwd: {}", cwd.display());
    println!("conf: {}", conf_path.display());
    let state = conf::load_server_config(&conf_path, &cwd).map_err(|e| e.to_string())?;
    let settings = match state {
        ServerConfigState::Ok(settings) => settings,
        ServerConfigState::Missing => {
            return Err(format!(
                "configuration file '{}' does not exist",
                conf_path.display()
            ))
        }
        ServerConfigState::Invalid { message } => {
            return Err(format!(
                "invalid configuration '{}': {message}",
                conf_path.display()
            ))
        }
    };

    let pre_session = pre_session_interaction();
    let api_key_override = match api_key {
        Some(raw) => Some(session::resolve_api_key_override(&settings.cwd, &raw)?),
        None => None,
    };

    let resolved = session::resolve_agent_for_run_interactive(
        &settings,
        &pre_session,
        api_key_override,
    )
    .await
    .map_err(|e| e.to_string())?;
    let agent = resolved.agent.clone();
    let config_path = resolved.config_path.clone();

    let source = session::source_with_agent_config(&settings, &config_path);
    let session_dir = agent_loop::session_dir(
        &settings.cwd,
        &settings.agent_session.log_folder,
        &agent.vendor,
        &agent.name,
    )?;
    let log_file = session_dir.join("log.md");
    let dump_file = session_dir.join("dump.json");
    let interaction = session_interaction(&log_file)?;

    let loop_ = agent_loop::AgentLoop::new(
        settings.cwd.clone(),
        settings.agent_session.log_folder.clone(),
        agent.clone(),
        settings.tools.clone(),
        interaction,
        session_dir,
    )?;
    let mut loop_ = loop_.with_dump_settings(source);
    let outcome = loop_
        .run(prompt, &dump_file, None, CancellationToken::new(), None)
        .await?;
    handle_outcome(outcome)
}

async fn start_resume_session(
    dump_path: PathBuf,
    api_key: Option<String>,
) -> Result<(), String> {
    let dump_path = session::resolve_abs_path(&dump_path)?;
    let (settings_value, cwd) = agent_loop::dump_settings::extract(&dump_path)?;
    println!("cwd: {}", cwd.display());
    println!("settings: from dump");

    std::fs::metadata(&cwd)
        .map_err(|e| format!("cannot access cwd '{}' from dump: {e}", cwd.display()))?;
    if !cwd.is_dir() {
        return Err(format!(
            "cwd '{}' from dump is not a directory",
            cwd.display()
        ));
    }

    let mut settings = conf::load_from_value(settings_value, &cwd, |_| {
        Err("agent was not selected".to_string())
    })
    .map_err(|e| e.to_string())?;

    if let Some(raw) = api_key {
        settings.agent.api_key = session::resolve_api_key_override(&settings.cwd, &raw)?;
    }

    let pre_session = pre_session_interaction();
    if settings.agent.api_key.is_empty() {
        settings.agent.api_key = session::prompt_api_key(
            &pre_session,
            &settings.agent.vendor,
            &settings.agent.name,
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    let session_dir = agent_loop::session_dir(
        &settings.cwd,
        &settings.log_folder,
        &settings.agent.vendor,
        &settings.agent.name,
    )?;
    let log_file = session_dir.join("log.md");
    let dump_file = session_dir.join("dump.json");
    let interaction = session_interaction(&log_file)?;

    let loop_ = agent_loop::AgentLoop::new(
        settings.cwd.clone(),
        settings.log_folder.clone(),
        settings.agent.clone(),
        settings.tools.clone(),
        interaction,
        session_dir,
    )?;
    let mut loop_ = loop_.with_dump_settings(settings.source.clone());
    let outcome = loop_
        .run(
            None,
            &dump_file,
            Some(&dump_path),
            CancellationToken::new(),
            None,
        )
        .await?;
    handle_outcome(outcome)
}

fn handle_outcome(outcome: agent_loop::RunOutcome) -> Result<(), String> {
    match outcome.state {
        RunState::Failed => Err(outcome
            .error
            .unwrap_or_else(|| "agent loop failed".to_string())),
        _ => Ok(()),
    }
}

fn pre_session_interaction() -> Arc<dyn Interaction> {
    Arc::new(CliInteraction::new(InteractionLoggers::new(None)))
}

fn session_interaction(log_file: &Path) -> Result<Arc<dyn Interaction>, String> {
    let file: Arc<dyn Logger> = Arc::new(
        FileLogger::new(log_file)
            .map_err(|e| format!("cannot open log '{}': {e}", log_file.display()))?,
    );
    Ok(Arc::new(CliInteraction::new(InteractionLoggers::new(
        Some(file),
    ))))
}

/// Print embedded help and guarantee a trailing newline.
fn print_help(text: &str) -> Result<(), String> {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}
