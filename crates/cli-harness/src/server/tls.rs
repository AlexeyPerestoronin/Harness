//! TLS certificate resolution and self-signed certificate generation.
//!
//! User-supplied PEM files (CLI or `server.tls-cert` / `server.tls-key`) are
//! used as-is. Otherwise a self-signed certificate is generated once and
//! reused from an OS data directory outside the harness `cwd`.

use std::path::{Path, PathBuf};

use conf::ServerConfig;

/// Resolved PEM certificate and private key paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPaths {
    /// PEM certificate path.
    pub cert: PathBuf,
    /// PEM private key path.
    pub key: PathBuf,
}

/// Choose TLS artifacts for the given effective server configuration.
pub fn resolve_tls_paths(server: &ServerConfig) -> Result<TlsPaths, String> {
    match (&server.tls_cert, &server.tls_key) {
        (Some(cert), Some(key)) => Ok(TlsPaths {
            cert: cert.clone(),
            key: key.clone(),
        }),
        (None, None) => {
            let dir = certs_dir()?;
            ensure_self_signed(&dir)
        }
        _ => Err("server.tls-cert and server.tls-key must be set together".to_string()),
    }
}

/// OS data directory for generated TLS artifacts.
fn certs_dir() -> Result<PathBuf, String> {
    let base = data_dir()?;
    Ok(base.join("harness").join("certs"))
}

fn data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            "LOCALAPPDATA is not set; cannot locate the TLS cert directory".to_string()
        })?;
        Ok(PathBuf::from(local))
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| "HOME is not set; cannot locate the TLS cert directory".to_string())?;
        Ok(PathBuf::from(home)
            .join("Library")
            .join("Application Support"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg));
            }
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| "HOME is not set; cannot locate the TLS cert directory".to_string())?;
        Ok(PathBuf::from(home).join(".local").join("share"))
    }
}

/// Generate `cert.pem` / `key.pem` in `dir` once and reuse them afterwards.
fn ensure_self_signed(dir: &Path) -> Result<TlsPaths, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("cannot create TLS cert directory '{}': {e}", dir.display()))?;
    set_dir_private(dir);

    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");

    if !cert_path.is_file() || !key_path.is_file() {
        let rcgen::CertifiedKey { cert, key_pair } = rcgen::generate_simple_self_signed(vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ])
        .map_err(|e| format!("cannot generate self-signed certificate: {e}"))?;

        std::fs::write(&cert_path, cert.pem())
            .map_err(|e| format!("cannot write '{}': {e}", cert_path.display()))?;
        std::fs::write(&key_path, key_pair.serialize_pem())
            .map_err(|e| format!("cannot write '{}': {e}", key_path.display()))?;
        set_key_private(&key_path);
    }

    Ok(TlsPaths {
        cert: cert_path,
        key: key_path,
    })
}

#[cfg(unix)]
fn set_dir_private(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_dir_private(_dir: &Path) {}

#[cfg(unix)]
fn set_key_private(key: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(key, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_key_private(_key: &Path) {}

