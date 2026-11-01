//! Prompt and answer parsing for an interactive agent choice.
//!
//! When `agent.config` is a directory with several candidates the caller
//! renders a selection prompt through [`build_prompt`] and parses the 1-based
//! answer with [`parse_choice`]. The interaction loop itself lives in
//! `cli-harness` so console and server runs share the same behaviour. The
//! prompt table keeps the legacy console format (`№`, `Vendor`, `Model`,
//! `Cost`, `Limits`).

use std::path::Path;

use conf::AgentCandidate;

/// Maximum number of invalid selection answers before failing.
pub const MAX_ATTEMPTS: u32 = 3;

/// Build the selection prompt with the legacy table layout.
pub fn build_prompt(candidates: &[AgentCandidate]) -> String {
    let dir = candidates[0]
        .path
        .parent()
        .unwrap_or_else(|| Path::new("."));

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

    let mut prompt = String::new();
    prompt.push_str(&format!("Available agents in {}:\n", dir.display()));
    prompt.push_str(&render_table_row(&headers, &widths));
    prompt.push('\n');
    prompt.push_str(&render_table_separator(&widths));
    prompt.push('\n');
    for row in &rows {
        prompt.push_str(&render_table_row(row, &widths));
        prompt.push('\n');
    }
    prompt.push('\n');
    prompt.push_str(&format!("Select agent [1-{}]: ", candidates.len()));
    prompt
}

/// Parse a 1-based agent number into a 0-based index.
pub fn parse_choice(input: &str, candidates_len: usize) -> Option<usize> {
    let parsed = input.trim().parse::<usize>().ok()?;
    if parsed >= 1 && parsed <= candidates_len {
        Some(parsed - 1)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_choice_accepts_one_based_numbers() {
        assert_eq!(parse_choice("1", 3), Some(0));
        assert_eq!(parse_choice("3", 3), Some(2));
        assert_eq!(parse_choice(" 2 ", 3), Some(1));
    }

    #[test]
    fn parse_choice_rejects_invalid_numbers() {
        assert_eq!(parse_choice("0", 3), None);
        assert_eq!(parse_choice("4", 3), None);
        assert_eq!(parse_choice("abc", 3), None);
    }
}
