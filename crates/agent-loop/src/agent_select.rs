//! Interactive choice of an agent file when `agent.config` is a directory.
//!
//! `conf` only scans and loads; this module talks to the [`Interaction`]
//! contract. The prompt table keeps the old console format (`№`, `Vendor`,
//! `Model`, `Cost`, `Limits`) and parses a 1-based number.

use std::path::Path;
use std::sync::Arc;

use conf::AgentCandidate;
use interactions::{Interaction, InteractionKind, InteractionRequest};

/// Maximum number of invalid selection answers before failing.
pub const MAX_ATTEMPTS: u32 = 3;

/// Ask the user to pick a candidate. Returns a 0-based index.
pub async fn choose(
    interaction: &Arc<dyn Interaction>,
    candidates: &[AgentCandidate],
) -> Result<usize, String> {
    if candidates.is_empty() {
        return Err("agent was not selected".into());
    }
    if candidates.len() == 1 {
        let c = &candidates[0];
        let line = format!(
            "Agent: {} {} ({})",
            c.vendor,
            c.name,
            file_name(&c.path)
        );
        let _ = interaction
            .output(&line, true, interactions::OutputType::NewLine)
            .await;
        return Ok(0);
    }

    let prompt = build_prompt(candidates);
    for _ in 0..MAX_ATTEMPTS {
        let request = InteractionRequest {
            kind: InteractionKind::Text,
            prompt: prompt.clone(),
            sensitive: false,
        };
        let answer = interaction
            .input(&request, true)
            .await
            .map_err(|e| format!("cannot select agent → {e}"))?;
        if let Some(index) = parse_choice(answer.trim(), candidates.len()) {
            return Ok(index);
        }
    }
    Err("agent was not selected".into())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(index: u8) -> AgentCandidate {
        AgentCandidate {
            path: Path::new("/tmp/agents").join(format!("agent-{index}.json5")),
            vendor: format!("Vendor{index}"),
            name: format!("Model{index}"),
            token_limit: 1000,
            rpm: -1,
            tpm: -1,
            rpd: -1,
            input_price: -1.0,
            output_price: -1.0,
            cached_price: -1.0,
        }
    }

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

    #[tokio::test]
    async fn choose_exhausts_attempts() {
        use interactions::{InteractionError, OutputType};
        use std::sync::Mutex;

        #[derive(Clone)]
        struct MockInteraction {
            answers: Arc<Mutex<Vec<String>>>,
        }

        #[async_trait::async_trait]
        impl Interaction for MockInteraction {
            async fn input(
                &self,
                _request: &InteractionRequest,
                _write_log: bool,
            ) -> Result<String, InteractionError> {
                Ok(self.answers.lock().unwrap().remove(0))
            }

            async fn output(
                &self,
                _message: &str,
                _write_log: bool,
                _new_line: OutputType,
            ) -> Result<(), InteractionError> {
                Ok(())
            }
        }

        let interaction: Arc<dyn Interaction> = Arc::new(MockInteraction {
            answers: Arc::new(Mutex::new(vec![
                "0".to_string(),
                "99".to_string(),
                "abc".to_string(),
            ])),
        });
        let candidates = vec![candidate(1), candidate(2)];
        let result = choose(&interaction, &candidates).await;
        assert_eq!(result.unwrap_err(), "agent was not selected");
    }
}
