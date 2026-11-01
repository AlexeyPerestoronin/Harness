//! HTTPS server mode for `harness.exe --serve`.
//!
//! The module wires CLI options, configuration loading, TLS artifacts,
//! authentication, CORS and the axum router into a single `serve` entry
//! point. The actual HTTP contract is implemented in [`handlers`]; the
//! asynchronous agent session is launched by [`run`].

pub mod handlers;
pub mod run;
pub mod state;
pub mod tls;
pub mod web_interaction;

use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use auth::AuthToken;
use conf::ServerConfigState;
use harness_api::ConfigState;
use tokio::sync::{Mutex, RwLock};

use self::state::{LoadedServerConfig, ServerState};

/// Server-mode options produced by [`crate::cli::parse_args`].
pub struct ServeOptions {
    /// Path to `harness.json5`.
    pub conf: PathBuf,
    /// Workspace override.
    pub cwd: Option<PathBuf>,
    /// CLI port override.
    pub port: Option<u16>,
    /// CLI bind host override.
    pub host: Option<String>,
    /// CLI TLS certificate override.
    pub tls_cert: Option<PathBuf>,
    /// CLI TLS private key override.
    pub tls_key: Option<PathBuf>,
    /// Fixed bearer token override.
    pub auth_token: Option<String>,
}

/// Start the HTTPS server and block until it stops.
pub async fn serve(options: ServeOptions) -> Result<(), String> {
    let loaded = conf::load_server_config(&options.conf).map_err(|e| e.to_string())?;
    let (config_state, mut settings, mut server) = match loaded {
        ServerConfigState::Ok(settings) => {
            let server = settings.server.clone();
            (ConfigState::Ok, Some(settings), server)
        }
        ServerConfigState::Missing => (ConfigState::Missing, None, conf::ServerConfig::default()),
        ServerConfigState::Invalid { message } => {
            eprintln!(
                "warning: invalid configuration '{}': {message}",
                options.conf.display()
            );
            (ConfigState::Invalid, None, conf::ServerConfig::default())
        }
    };

    if let Some(raw) = options.cwd {
        let conf_dir = settings
            .as_ref()
            .map(|s| s.conf_dir.clone())
            .unwrap_or_else(|| {
                options
                    .conf
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });
        let cwd = resolve_cwd_override(&raw, &conf_dir)?;
        if let Some(settings) = settings.as_mut() {
            settings.cwd = cwd;
        }
    }

    if let Some(host) = options.host {
        server.host = host;
    }
    if let Some(port) = options.port {
        server.port = port;
    }
    if let Some(cert) = options.tls_cert {
        server.tls_cert = Some(cert);
    }
    if let Some(key) = options.tls_key {
        server.tls_key = Some(key);
    }

    let auth_token = match options.auth_token {
        Some(raw) => AuthToken::parse(&raw)
            .map_err(|_| "invalid --auth-token: expected a 32-byte base64url token".to_string())?,
        None => AuthToken::generate(),
    };
    println!("{}", auth_token.as_base64url());

    let live_timeout = Duration::from_secs(server.live_timeout_minutes.saturating_mul(60));
    let state = Arc::new(ServerState {
        config: RwLock::new(LoadedServerConfig {
            path: options.conf.clone(),
            config_state,
            settings,
            server: server.clone(),
        }),
        active: Mutex::new(None),
        auth_token,
        live_timeout,
        live_deadline: Mutex::new(Instant::now() + live_timeout),
    });

    let tls_paths = tls::resolve_tls_paths(&server)?;
    let app = handlers::router(state.clone());
    let addr = resolve_addr(&server.host, server.port).await?;

    tokio::spawn(live_timeout_task(state.clone()));

    let cert_pem = tokio::fs::read(&tls_paths.cert).await.map_err(|e| {
        format!(
            "cannot read TLS certificate '{}': {e}",
            tls_paths.cert.display()
        )
    })?;
    let key_pem = tokio::fs::read(&tls_paths.key)
        .await
        .map_err(|e| format!("cannot read TLS key '{}': {e}", tls_paths.key.display()))?;
    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem, key_pem)
        .await
        .map_err(|e| format!("cannot load TLS certificate/key: {e}"))?;

    axum_server::bind_rustls(addr, rustls_config)
        .serve(app.into_make_service())
        .await
        .map_err(|e| format!("server error: {e}"))
}

async fn resolve_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    let mut addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("cannot resolve host '{host}': {e}"))?;
    addrs
        .next()
        .ok_or_else(|| format!("host '{host}' resolved to no addresses"))
}

async fn live_timeout_task(state: Arc<ServerState>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;

        let is_active = {
            let active = state.active.lock().await;
            active
                .as_ref()
                .map(|run| crate::server::state::is_active(run.state))
                .unwrap_or(false)
        };
        if is_active {
            continue;
        }

        let deadline = *state.live_deadline.lock().await;
        if Instant::now() >= deadline {
            std::process::exit(0);
        }
    }
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
