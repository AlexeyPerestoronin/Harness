//! Vendor-specific HTTP agents (xAI, Google, Claude, DeepSeek and AliBaba).
//!
//! There is no official Rust SDK; they talk REST. The [`Agent`] trait matches
//! Python `IAgent`: one iteration, finish/stats, dump/reload. The API key is
//! already resolved by `conf` and is never read from a hardcoded filename.

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::{ToolDescriptor, ToolRegistry};

mod alibaba;
mod claude;
mod deepseek;
mod google;
mod openai_compat;
mod xai;

pub use alibaba::AlibabaAgent;
pub use claude::ClaudeAgent;
pub use deepseek::DeepSeekAgent;
pub use google::GoogleAgent;
pub use xai::XaiAgent;

/// Contract implemented by vendor agents.
pub trait Agent {
    /// Vendor identifier (`SpaceXAI` / `GoogleAI` / `Claude` / `DeepSeek` / `AliBaba`).
    fn vendor(&self) -> &str;
    /// Display model name from the conf.
    fn model(&self) -> &str;
    /// Session token limit (`-1` = unlimited).
    fn tokens_limit(&self) -> i64;
    /// Tokens consumed so far.
    fn consumed_tokens(&self) -> i64;
    /// Session USD limit (`-1` = unlimited).
    fn limit_usd(&self) -> f64;
    /// Approximated USD spent so far (tokens × model prices).
    fn consumed_usd(&self) -> f64;
    /// Vendor-reported USD so far. `None` if no response had a cost field.
    fn consumed_usd_real(&self) -> Option<f64>;
    /// Run one model turn. `true` = session complete (no tool calls).
    fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String>;
    /// Write usage statistics to the logger.
    fn finish(&self);
    /// Snapshot session state to `path`.
    fn dump_session(&self, path: &Path) -> Result<(), String>;
    /// Restore session state from `path`.
    fn reload_session(&mut self, path: &Path) -> Result<(), String>;
}

/// Shared usage counters stored in dump files.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UsageStats {
    /// Number of successful model HTTP calls.
    pub requests: u64,
    /// Prompt tokens.
    pub input_tokens: i64,
    /// Completion tokens.
    pub output_tokens: i64,
    /// Cached prompt tokens (xAI / Google / DeepSeek / AliBaba / Claude).
    pub cached_tokens: i64,
    /// Accumulated approximated USD cost (tokens × model prices).
    pub total_cost_usd: f64,
    /// Vendor-reported USD from response bodies, when present.
    #[serde(default)]
    pub real_cost_usd: Option<f64>,
}

/// Human-readable value used when no response in the session had a cost field.
pub(crate) const USD_REAL_UNKNOWN: &str = "doesn't obtained from response";

/// One vendor-reported cost observation parsed from a response body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RealCost {
    /// Cost of this request: add it to the accumulated value.
    Add(f64),
    /// Cumulative cost reported by the vendor: replace the accumulated value.
    Replace(f64),
}

/// Approximate USD for one successful response.
///
/// `input_tokens` is the full input including cache; `cached_tokens` is the
/// cache subset. Negative prices mean "free" and contribute `0`.
pub(crate) fn approximate_usd(
    input_tokens: i64,
    cached_tokens: i64,
    output_tokens: i64,
    input_price: f64,
    output_price: f64,
    cached_price: f64,
) -> f64 {
    let input_price = if input_price < 0.0 { 0.0 } else { input_price };
    let output_price = if output_price < 0.0 {
        0.0
    } else {
        output_price
    };
    let cached_price = if cached_price < 0.0 {
        0.0
    } else {
        cached_price
    };
    let cached = cached_tokens.max(0);
    let uncached = (input_tokens - cached).max(0);
    (uncached as f64) / 1_000_000.0 * input_price
        + (cached as f64) / 1_000_000.0 * cached_price
        + (output_tokens.max(0) as f64) / 1_000_000.0 * output_price
}

/// Parse the first vendor-reported cost from a response body.
///
/// Looks at the `usage` object (the caller passes its vendor-specific usage
/// object) and the root, in that order. Keys are checked in one priority:
/// `cost_in_usd_ticks`, then per-request `cost_in_usd` / `cost_usd` / `cost`,
/// then cumulative `total_cost_usd` / `total_cost`.
pub(crate) fn real_cost_from_body(
    root: &serde_json::Value,
    usage: &serde_json::Value,
) -> Option<RealCost> {
    for src in [usage, root] {
        if let Some(ticks) = src.get("cost_in_usd_ticks").and_then(json_f64) {
            const TICKS_PER_USD: f64 = 10_000_000_000.0;
            return Some(RealCost::Add(ticks / TICKS_PER_USD));
        }
    }
    for src in [usage, root] {
        for key in ["cost_in_usd", "cost_usd", "cost"] {
            if let Some(v) = src.get(key).and_then(json_f64) {
                return Some(RealCost::Add(v));
            }
        }
    }
    for src in [usage, root] {
        for key in ["total_cost_usd", "total_cost"] {
            if let Some(v) = src.get(key).and_then(json_f64) {
                return Some(RealCost::Replace(v));
            }
        }
    }
    None
}

/// Apply a parsed real-cost observation to the accumulated session value.
pub(crate) fn accumulate_real_cost(current: Option<f64>, update: Option<RealCost>) -> Option<f64> {
    match update {
        None => current,
        Some(RealCost::Add(v)) => Some(current.unwrap_or(0.0) + v),
        Some(RealCost::Replace(v)) => Some(v),
    }
}

/// Format USD for logs: no exponent, at least two decimal places for values
/// with `|usd| >= 0.01` or `0`, up to six for small values without trailing
/// zeros (so `0.00015` does not collapse to `0.00`).
pub(crate) fn format_usd(value: f64) -> String {
    if value == 0.0 {
        return "0.00".to_string();
    }
    let abs = value.abs();
    if abs >= 0.01 {
        return format!("{value:.2}");
    }
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() || text == "-" {
        "0.00".to_string()
    } else {
        text
    }
}

/// Format real cost or the session-wide "not obtained" string.
pub(crate) fn format_real_cost(value: Option<f64>) -> String {
    match value {
        Some(v) => format_usd(v),
        None => USD_REAL_UNKNOWN.to_string(),
    }
}

fn json_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_u64().map(|n| n as f64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Convert registry descriptors into OpenAI-style tool definitions.
///
/// OpenAI-compatible vendors (DeepSeek, xAI, Qwen) require
/// `function.parameters` to be a JSON Schema with `type: "object"`.
/// No-arg tools may be `{}` in the registry; those get `type: object`
/// so DeepSeek does not 400 with `got 'type: null'`.
pub fn openai_tools(list: &[ToolDescriptor]) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": object_json_schema(&t.parameters),
                    }
                })
            })
            .collect(),
    )
}

/// Convert registry descriptors into Gemini `functionDeclarations`.
pub fn gemini_tools(list: &[ToolDescriptor]) -> serde_json::Value {
    let declarations: Vec<_> = list
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            })
        })
        .collect();
    serde_json::json!([{ "functionDeclarations": declarations }])
}

/// Convert registry descriptors into Anthropic tool definitions.
///
/// Anthropic requires `input_schema.type`. No-arg tools may be `{}` in the
/// registry; those get `type: object` so the Messages API does not 400.
pub fn claude_tools(list: &[ToolDescriptor]) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": object_json_schema(&t.parameters),
                })
            })
            .collect(),
    )
}

/// Ensure a JSON Schema object has `type: "object"`.
///
/// DeepSeek and Claude reject no-arg tools whose schema is `{}` (they
/// report `type: null`). A non-object value becomes `{ "type": "object" }`.
fn object_json_schema(parameters: &serde_json::Value) -> serde_json::Value {
    let mut schema = parameters.clone();
    match schema.as_object_mut() {
        Some(obj) => {
            if !obj.contains_key("type") {
                obj.insert("type".into(), serde_json::json!("object"));
            }
            schema
        }
        None => serde_json::json!({"type": "object"}),
    }
}

/// Dispatch a tool call and return the result string (including execution errors).
pub fn invoke_tool(
    tools: &ToolRegistry,
    logger: &Arc<dyn Logger>,
    name: &str,
    args: serde_json::Value,
) -> String {
    match tools.call(name, args) {
        Ok(result) => result,
        Err(e) => {
            let msg = format!("execution error: {e}");
            logger.log_line(&format!("❌ fail → {msg}"));
            msg
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn approximate_usd_matches_task_examples() {
        // input=1_000_000, cached=200_000, output=500_000, prices 2 / 10 / 0.1
        assert_close(
            approximate_usd(1_000_000, 200_000, 500_000, 2.0, 10.0, 0.1),
            6.62,
        );
        // cached price -1 = free: cache contributes 0, so 6.60
        assert_close(
            approximate_usd(1_000_000, 200_000, 500_000, 2.0, 10.0, -1.0),
            6.60,
        );
        // all prices -1 = free
        assert_close(
            approximate_usd(1_000_000, 200_000, 500_000, -1.0, -1.0, -1.0),
            0.0,
        );
        // negative input/output prices also give zero contribution
        assert_close(approximate_usd(1_000_000, 0, 500_000, -1.0, -1.0, 0.1), 0.0);
    }

    #[test]
    fn real_cost_ticks_is_an_add_update() {
        let body = json!({"usage": {"cost_in_usd_ticks": 10000000000u64}});
        let usage = body.get("usage").unwrap();
        assert_eq!(real_cost_from_body(&body, usage), Some(RealCost::Add(1.0)));
    }

    #[test]
    fn real_cost_missing_keys_is_none() {
        let body = json!({"usage": {"prompt_tokens": 10}});
        let usage = body.get("usage").unwrap();
        assert_eq!(real_cost_from_body(&body, usage), None);
        assert_eq!(format_real_cost(None), USD_REAL_UNKNOWN);
    }

    #[test]
    fn real_cost_explicit_zero_is_some_zero() {
        let body = json!({"usage": {"cost_usd": 0}});
        let usage = body.get("usage").unwrap();
        assert_eq!(real_cost_from_body(&body, usage), Some(RealCost::Add(0.0)));
        assert_eq!(format_real_cost(Some(0.0)), "0.00");
    }

    #[test]
    fn real_cost_accumulates_per_request_values() {
        let first_body = json!({"usage": {"cost_usd": 1.5}});
        let first_update = real_cost_from_body(&first_body, first_body.get("usage").unwrap());
        assert_eq!(first_update, Some(RealCost::Add(1.5)));
        let first = accumulate_real_cost(None, first_update);
        assert_close(first.unwrap(), 1.5);

        let second_body = json!({"usage": {"cost_usd": 0.5}});
        let second_update = real_cost_from_body(&second_body, second_body.get("usage").unwrap());
        let second = accumulate_real_cost(first, second_update);
        assert_close(second.unwrap(), 2.0);
    }

    #[test]
    fn real_cost_total_replaces_accumulated_value() {
        let first_body = json!({"usage": {"cost_usd": 1.5}});
        let first = accumulate_real_cost(
            None,
            real_cost_from_body(&first_body, first_body.get("usage").unwrap()),
        );
        assert_close(first.unwrap(), 1.5);

        let second_body = json!({"usage": {"total_cost": 1.25}});
        let second = accumulate_real_cost(
            first,
            real_cost_from_body(&second_body, second_body.get("usage").unwrap()),
        );
        assert_close(second.unwrap(), 1.25);
    }

    #[test]
    fn format_usd_keeps_small_values_visible() {
        assert_eq!(format_usd(10.34), "10.34");
        assert_eq!(format_usd(0.0), "0.00");
        assert_eq!(format_usd(0.00015), "0.00015");
    }
}
