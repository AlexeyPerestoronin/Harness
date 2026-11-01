//! Native `harness.exe` entry point.
//!
//! Wires CLI → conf crate → asynchronous agent loop. Configuration files are
//! opened only inside `conf`; this binary receives typed settings.

mod cli;
mod server;

use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use agent_loop::RunState;
use cli::{parse_args, read_prompt_file, version_line, CliAction, HELP};
use conf::{ServerConfigState, ServerSettings};
use interactions::{
    CliInteraction, Interaction, InteractionKind, InteractionLoggers, InteractionRequest,
};
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
                (Some(dump_path), _) => start_resume_session(dump_path, cwd, api_key).await,
                (None, Some(conf_path)) => start_new_session(conf_path, prompt, cwd, api_key).await,
                (None, None) => Err("missing --conf".to_string()),
            }
        }
    }
}

async fn start_new_session(
    conf_path: PathBuf,
    prompt: Option<String>,
    cwd: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<(), String> {
    let state = conf::load_server_config(&conf_path).map_err(|e| e.to_string())?;
    let mut settings = match state {
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

    if let Some(raw) = cwd {
        settings.cwd = resolve_cwd_override(&raw, &settings.conf_dir)?;
    }

    let pre_session = pre_session_interaction();
    let api_key_override = match api_key {
        Some(raw) => Some(resolve_api_key_override(&settings.cwd, &raw)?),
        None => None,
    };

    let resolved =
        resolve_agent_for_run_interactive(&settings, &pre_session, api_key_override).await?;
    let agent = resolved.agent.clone();
    let config_path = resolved.config_path.clone();

    let source = source_with_agent_config(&settings, &config_path);
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
    cwd: Option<PathBuf>,
    api_key: Option<String>,
) -> Result<(), String> {
    let settings_value = agent_loop::dump_settings::extract(&dump_path)?;
    let conf_dir = agent_loop::dump_settings::conf_dir_of(&settings_value)?;
    let mut settings = conf::load_from_value(settings_value, &conf_dir, cwd, |_| {
        Err("agent was not selected".to_string())
    })
    .map_err(|e| e.to_string())?;

    if let Some(raw) = api_key {
        settings.agent.api_key = resolve_api_key_override(&settings.cwd, &raw)?;
    }

    let pre_session = pre_session_interaction();
    if settings.agent.api_key.is_empty() {
        settings.agent.api_key =
            prompt_api_key(&pre_session, &settings.agent.vendor, &settings.agent.name).await?;
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

async fn resolve_agent_for_run_interactive(
    settings: &ServerSettings,
    interaction: &Arc<dyn Interaction>,
    mut api_key_override: Option<String>,
) -> Result<conf::ResolvedAgent, String> {
    let mut choice: Option<String> = None;
    loop {
        match conf::resolve_agent_for_run(settings, choice.as_deref(), api_key_override.as_deref())
        {
            Ok(resolved) => {
                if resolved.agent.api_key.is_empty() && api_key_override.is_none() {
                    let key =
                        prompt_api_key(interaction, &resolved.agent.vendor, &resolved.agent.name)
                            .await?;
                    api_key_override = Some(key);
                    continue;
                }
                return Ok(resolved);
            }
            Err(e) if e.is_agent_choice_required() => {
                let candidates = match e.kind() {
                    conf::ConfErrorKind::AgentChoiceRequired(candidates) => candidates.clone(),
                    _ => return Err(e.to_string()),
                };
                let index = agent_loop::agent_select::choose(interaction, &candidates).await?;
                choice = Some((index + 1).to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

async fn prompt_api_key(
    interaction: &Arc<dyn Interaction>,
    vendor: &str,
    name: &str,
) -> Result<String, String> {
    let request = InteractionRequest {
        kind: InteractionKind::Text,
        prompt: format!("Enter API key for {vendor} {name}: "),
        sensitive: true,
    };
    let key = interaction
        .input(&request, false)
        .await
        .map_err(|e| format!("cannot prompt for api key: {e}"))?;
    if key.is_empty() {
        return Err("api-key is required".to_string());
    }
    Ok(key)
}

fn resolve_api_key_override(cwd: &Path, raw: &str) -> Result<String, String> {
    let candidate = PathBuf::from(raw);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    };
    if path.is_file() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read api-key file '{}': {e}", path.display()))?;
        Ok(contents.trim().to_string())
    } else {
        Ok(raw.to_string())
    }
}

fn source_with_agent_config(settings: &ServerSettings, config_path: &Path) -> serde_json::Value {
    let mut source = settings.source.clone();
    if let Some(agent) = source
        .as_object_mut()
        .and_then(|root| root.get_mut("agent"))
        .and_then(|agent| agent.as_object_mut())
    {
        agent.insert(
            "config".to_string(),
            serde_json::Value::String(config_path.to_string_lossy().into_owned()),
        );
    }
    source
}

fn resolve_cwd_override(raw: &Path, conf_dir: &Path) -> Result<PathBuf, String> {
    let base = if conf_dir.is_absolute() {
        conf_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(conf_dir)
    };
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base.join(raw)
    };
    normalize_abs(&absolute)
}

fn normalize_abs(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("'cwd' must be an absolute path".to_string());
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    if normalized.as_os_str().is_empty() {
        Err("'cwd' must be set explicitly".to_string())
    } else {
        Ok(normalized)
    }
}

/// Print embedded help and guarantee a trailing newline.
fn print_help(text: &str) -> Result<(), String> {
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn api_key_prompt_does_not_write_key_to_log() {
        let dir = std::env::temp_dir().join(format!("harness-cli-key-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("log.md");
        let file: Arc<dyn Logger> = Arc::new(FileLogger::new(&log_path).unwrap());
        let writer = Vec::<u8>::new();
        let interaction: Arc<dyn Interaction> = Arc::new(CliInteraction::with_reader_writer(
            InteractionLoggers::new(Some(file)),
            Cursor::new(b"secret-key\r\n".to_vec()),
            writer,
        ));

        let key = prompt_api_key(&interaction, "Vendor", "Model")
            .await
            .unwrap();
        assert_eq!(key, "secret-key");

        let logged = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(!logged.contains("secret-key"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
