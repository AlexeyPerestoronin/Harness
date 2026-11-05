//! HTTPS server mode for `harness.exe --serve`.
//!
//! The module wires CLI options, configuration loading, TLS artifacts,
//! authentication, CORS and the axum router into a single `serve` entry
//! point. The actual HTTP contract is implemented in [`handlers`]; the
//! asynchronous agent session is launched by [`run`].

pub mod config;
pub mod fs_api;
pub mod handlers;
pub mod run;
mod secret;
pub mod state;
pub mod tls;
pub mod web_interaction;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use auth::AuthToken;
use conf::ServerConfigState;
use harness_api::ConfigState;
use tokio::sync::{Mutex, RwLock};

use crate::session;
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
    let cwd = session::resolve_cwd(options.cwd.as_deref())?;
    let conf_path = session::resolve_abs_path(&options.conf)?;
    let loaded = conf::load_server_config(&conf_path, &cwd).map_err(|e| e.to_string())?;
    let (config_state, settings, mut server) = match loaded {
        ServerConfigState::Ok(settings) => {
            let server = settings.server.clone();
            (ConfigState::Ok, Some(settings), server)
        }
        ServerConfigState::Missing => (ConfigState::Missing, None, conf::ServerConfig::default()),
        ServerConfigState::Invalid { message } => {
            eprintln!(
                "warning: invalid configuration '{}': {message}",
                conf_path.display()
            );
            (ConfigState::Invalid, None, conf::ServerConfig::default())
        }
    };

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
            path: conf_path,
            cwd: cwd.clone(),
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

    print_startup_info(&state, &server, &tls_paths).await;

    let app = handlers::router(state.clone());
    let initial_addr = resolve_addr(&server.host, server.port).await?;
    let addr = find_free_port(initial_addr.ip(), server.port).await?;
    println!("listening on https://{}:{}", server.host, addr.port());

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

/// Print the server startup information before the live-timeout task and bind.
async fn print_startup_info(
    state: &Arc<ServerState>,
    server: &conf::ServerConfig,
    tls_paths: &tls::TlsPaths,
) {
    let config = state.config.read().await;
    println!("{} (server mode)", crate::cli::version_line());
    println!("conf: {}", config.path.display());
    println!("cwd: {}", config.cwd.display());
    println!("host: {}", server.host);
    println!("port: {}", server.port);
    match (&server.tls_cert, &server.tls_key) {
        (Some(cert), Some(key)) => {
            println!("tls: cert={} key={}", cert.display(), key.display());
        }
        _ => {
            let tls_dir = tls_paths.cert.parent().unwrap_or(tls_paths.cert.as_path());
            println!("tls: auto-generated in {}", tls_dir.display());
        }
    }
    println!("live-timeout: {}m", server.live_timeout_minutes);
    println!("interaction-timeout: {}m", server.interaction_timeout_minutes);
    if server.cors_allowed_origins.is_empty() {
        println!("CORS disabled");
    } else {
        println!("CORS allowlist: {}", server.cors_allowed_origins.join(", "));
    }
}

/// Find a free TCP port starting from `start_port` and return the bind address.
async fn find_free_port(ip: IpAddr, start_port: u16) -> Result<SocketAddr, String> {
    let mut port = start_port;
    loop {
        match tokio::net::TcpListener::bind(SocketAddr::new(ip, port)).await {
            Ok(listener) => {
                drop(listener);
                if port != start_port {
                    println!("selected free port {port}");
                }
                return Ok(SocketAddr::new(ip, port));
            }
            Err(_) if port == u16::MAX => {
                return Err(format!("no free port found from {start_port} to 65535"));
            }
            Err(_) => {
                println!("port {port} is already in use, trying {}...", port + 1);
                port += 1;
            }
        }
    }
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
