//! Vendor-specific HTTP agents (xAI Grok and Google Gemini).
//!
//! There is no official Rust SDK; both talk REST. The [`Agent`] trait matches
//! Python `IAgent`: one iteration, finish/stats, dump/reload. The API key is
//! already resolved by `conf` and is never read from a hardcoded filename.

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::{ToolDescriptor, ToolRegistry};

mod google;
mod xai;

pub use google::GoogleAgent;
pub use xai::XaiAgent;

/// Contract implemented by vendor agents.
pub trait Agent {
    /// Vendor identifier (`SpaceXAI` / `GoogleAI`).
    fn vendor(&self) -> &str;
    /// Display model name from the conf.
    fn model(&self) -> &str;
    /// Session token limit (`-1` = unlimited).
    fn tokens_limit(&self) -> i64;
    /// Tokens consumed so far.
    fn consumed_tokens(&self) -> i64;
    /// Session USD limit (`-1` = unlimited).
    fn limit_usd(&self) -> f64;
    /// USD spent so far.
    fn consumed_usd(&self) -> f64;
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
    /// Cached prompt tokens (xAI / Google).
    pub cached_tokens: i64,
    /// Accumulated USD cost when the vendor reports it.
    pub total_cost_usd: f64,
}

/// Convert registry descriptors into OpenAI-style tool definitions.
pub fn openai_tools(list: &[ToolDescriptor]) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
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
