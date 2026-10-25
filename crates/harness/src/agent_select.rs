//! Interactive choice of an agent file when `agent.config` is a directory.
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

    let headers = vec![
        "№".to_string(),
        "Vendor".to_string(),
        "Model".to_string(),
        "Cost (Input/Output/Cached)".to_string(),
        "Limits (Tokens/RPM/TPM/RPD)".to_string(),
    ];
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();

    let mut rows = Vec::with_capacity(candidates.len());
    for (i, candidate) in candidates.iter().enumerate() {
        let cells = vec![
            (i + 1).to_string(),
            candidate.vendor.clone(),
            candidate.name.clone(),
            format_cost(candidate),
            format_limits(candidate),
        ];
        for (col, cell) in cells.iter().enumerate() {
            widths[col] = widths[col].max(cell.chars().count());
        }
        rows.push(cells);
    }

    writeln!(output, "{}", render_table_row(&headers, &widths)).map_err(|e| e.to_string())?;
    writeln!(output, "{}", render_table_separator(&widths)).map_err(|e| e.to_string())?;
    for row in &rows {
        writeln!(output, "{}", render_table_row(row, &widths)).map_err(|e| e.to_string())?;
    }
    writeln!(output).map_err(|e| e.to_string())?;

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

fn render_table_row(cells: &[String], widths: &[usize]) -> String {
    let mut row = String::from("|");
    for (cell, width) in cells.iter().zip(widths) {
        row.push(' ');
        row.push_str(&format!("{:<width$}", cell, width = *width));
        row.push(' ');
        row.push('|');
    }
    row
}

fn render_table_separator(widths: &[usize]) -> String {
    let mut row = String::from("|");
    for width in widths {
        row.push_str(&"-".repeat(*width + 2));
        row.push('|');
    }
    row
}

fn format_cost(candidate: &AgentCandidate) -> String {
    format!(
        "{} / {} / {}",
        format_price(candidate.input_price),
        format_price(candidate.output_price),
        format_price(candidate.cached_price)
    )
}

fn format_limits(candidate: &AgentCandidate) -> String {
    format!(
        "{} / {} / {} / {}",
        candidate.token_limit, candidate.rpm, candidate.tpm, candidate.rpd
    )
}

fn format_price(value: f64) -> String {
    if value.to_bits() == (-1.0f64).to_bits() {
        return "-1".to_string();
    }

    let mut text = format!("{value}");
    match text.split_once('.') {
        Some((_, fraction)) if fraction.len() == 1 => text.push('0'),
        None => text.push_str(".00"),
        Some(_) => {}
    }
    text
}
