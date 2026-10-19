//! Vendor-specific HTTP agents (xAI Grok, Google Gemini and Anthropic Claude).
//!
//! There is no official Rust SDK; they talk REST. The [`Agent`] trait matches
//! Python `IAgent`: one iteration, finish/stats, dump/reload. The API key is
//! already resolved by `conf` and is never read from a hardcoded filename.

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::{ToolDescriptor, ToolRegistry};

mod claude;
mod google;
mod xai;

pub use claude::ClaudeAgent;
pub use google::GoogleAgent;
pub use xai::XaiAgent;

/// Contract implemented by vendor agents.
pub trait Agent {
    /// Vendor identifier (`SpaceXAI` / `GoogleAI` / `Claude`).
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
                    "input_schema": claude_input_schema(&t.parameters),
                })
            })
            .collect(),
    )
}

/// Ensure a JSON Schema object has `type` (required by Claude `input_schema`).
fn claude_input_schema(parameters: &serde_json::Value) -> serde_json::Value {
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

    #[test]
    fn claude_tools_adds_type_to_empty_schema() {
        let list = [ToolDescriptor {
            name: "list_available_file_extension".into(),
            description: "List of available file extensions.".into(),
            parameters: serde_json::json!({}),
        }];
        let tools = claude_tools(&list);
        assert_eq!(tools[0]["name"], "list_available_file_extension");
        assert_eq!(tools[0]["input_schema"]["type"], "object");
    }

    #[test]
    fn claude_tools_preserves_existing_schema() {
        let list = [ToolDescriptor {
            name: "read_file".into(),
            description: "Read a file.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        }];
        let tools = claude_tools(&list);
        assert_eq!(tools[0]["input_schema"]["type"], "object");
        assert_eq!(tools[0]["input_schema"]["required"][0], "path");
        assert_eq!(
            tools[0]["input_schema"]["properties"]["path"]["type"],
            "string"
        );
    }
}
