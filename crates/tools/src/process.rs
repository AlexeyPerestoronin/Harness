//! Process-launch helper shared by `terminal` and `user`.
//!
//! Not a tool module: it does not export descriptors and has no conf key.
//! Stdout of the child is streamed to the parent stdout (not through Logger).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Run `cmd` in `cwd` with a timeout, streaming output to stdout.
///
/// Empty captured output becomes `(command finished without output)`.
pub fn run_observed_command(cmd: &str, cwd: &Path, timeout_secs: u64) -> Result<String, String> {
    let mut command = shell_command(cmd);
    command
        .current_dir(cwd)
        .env("PYTHONUNBUFFERED", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot spawn '{cmd}': {e}"))?;

    let chunks = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut pumps = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        pumps.push(spawn_pump(stdout, chunks.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        pumps.push(spawn_pump(stderr, chunks.clone()));
    }

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                for t in pumps {
                    let _ = t.join();
                }
                return Err(format!(
                    "execution of the '{cmd}' exceed the limit (available limit is {timeout_secs}s)"
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                return Err(format!("wait failed for '{cmd}': {e}"));
            }
        }
    };
    for t in pumps {
        let _ = t.join();
    }
    let _ = status;
    let raw = chunks.lock().map(|g| g.clone()).unwrap_or_default();
    Ok(decode_command_output(&raw))
}

fn spawn_pump<R: Read + Send + 'static>(
    mut stream: R,
    chunks: Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = &buf[..n];
                    if let Ok(mut g) = chunks.lock() {
                        g.extend_from_slice(data);
                    }
                    let _ = std::io::stdout().write_all(data);
                    let _ = std::io::stdout().flush();
                }
                Err(_) => break,
            }
        }
    })
}

fn shell_command(cmd: &str) -> Command {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    }
}

fn decode_command_output(raw: &[u8]) -> String {
    if raw.is_empty() {
        return "(command finished without output)".to_string();
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string();
    }
    let oem = encoding_rs::IBM866.decode(raw);
    if !oem.2 {
        return oem.0.into_owned();
    }
    encoding_rs::WINDOWS_1251.decode(raw).0.into_owned()
}
