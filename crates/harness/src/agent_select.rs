//! Interactive choice of an agent file when `agent-setting.config` is a directory.
//!
//! `conf` only scans and loads; this module talks to stdin/stdout. Tests inject
//! [`BufRead`] / [`Write`] so the prompt logic does not need a real terminal.

use std::io::{self, BufRead, Write};
use std::path::Path;

use conf::AgentCandidate;

const MAX_ATTEMPTS: u32 = 3;

/// Ask the user to pick a candidate. Stdin/stdout variant used by `main`.
pub fn choose(candidates: &[AgentCandidate]) -> Result<usize, String> {
    choose_from(candidates, io::stdin().lock(), io::stdout())
}

/// Same as [`choose`], with injected reader/writer.
fn choose_from<R, W>(
    candidates: &[AgentCandidate],
    mut input: R,
    mut output: W,
) -> Result<usize, String>
where
    R: BufRead,
    W: Write,
{
    if candidates.is_empty() {
        return Err("agent was not selected".into());
    }
    if candidates.len() == 1 {
        let c = &candidates[0];
        writeln!(
            output,
            "Agent: {} {} ({})",
            c.vendor,
            c.name,
            file_name(&c.path)
        )
        .map_err(|e| e.to_string())?;
        return Ok(0);
    }

    let dir = candidates[0]
        .path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    writeln!(output, "Available agents in {}:", dir.display()).map_err(|e| e.to_string())?;

    let labels: Vec<String> = candidates
        .iter()
        .map(|c| format!("{} {}", c.vendor, c.name))
        .collect();
    let width = labels.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    for (i, (c, label)) in candidates.iter().zip(&labels).enumerate() {
        writeln!(
            output,
            "  {}) {:width$}  ({})",
            i + 1,
            label,
            file_name(&c.path),
            width = width
        )
        .map_err(|e| e.to_string())?;
    }

    let n = candidates.len();
    let prompt = format!("Select agent [1-{n}]: ");
    for _ in 0..MAX_ATTEMPTS {
        write!(output, "{prompt}").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        let bytes = input.read_line(&mut line).map_err(|e| e.to_string())?;
        if bytes == 0 {
            return Err("agent was not selected".into());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err("agent was not selected".into());
        }
        if let Ok(choice) = trimmed.parse::<usize>() {
            if (1..=n).contains(&choice) {
                return Ok(choice - 1);
            }
        }
    }
    Err("agent was not selected".into())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
