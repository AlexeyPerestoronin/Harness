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
    /// Cached prompt tokens (xAI / Google / DeepSeek / AliBaba).
    pub cached_tokens: i64,
    /// Accumulated USD cost when the vendor reports it.
    pub total_cost_usd: f64,
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
