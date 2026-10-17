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
pub fn claude_tools(list: &[ToolDescriptor]) -> serde_json::Value {
    serde_json::Value::Array(
        list.iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect(),
    )
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

/// Helpers for ignored live availability tests (manual run only, not CI).
#[cfg(test)]
pub(crate) mod live_support {
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use chrono::Local;
    use conf::{load_model, AgentSettings, Mode, ToolsSettings};
    use logger::{DoubleLogger, Logger};
    use tools::{ToolRegistry, ToolsContext};

    pub const PROMPT: &str = "Ты доступен? Какой ты AI?";

    fn read_api_key(vendor: &str) -> String {
        eprint!("API key for {vendor}: ");
        let _ = io::stderr().flush();
        let mut key = String::new();
        io::stdin()
            .read_line(&mut key)
            .expect("read API key from stdin");
        let key = key.trim().to_string();
        assert!(!key.is_empty(), "{vendor} API key must not be empty");
        key
    }

    fn workspace_root() -> PathBuf {
        let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        raw.canonicalize().unwrap_or(raw)
    }

    /// Load `./agents/{model_file}`, take the API key from stdin, log Q&A via DoubleLogger.
    pub fn prepare(model_file: &str) -> (AgentSettings, Arc<dyn Logger>, Arc<ToolRegistry>) {
        let model_path = workspace_root().join("agents").join(model_file);
        let mut settings = load_model(&model_path, Mode::AutoAccept).unwrap_or_else(|e| {
            panic!("cannot load agent file '{}': {e}", model_path.display())
        });
        settings.api_key = read_api_key(&settings.vendor);

        let stamp = Local::now().format("%Y-%m-%d %H-%M-%S");
        let log_file = workspace_root()
            .join(".log")
            .join("live")
            .join(format!(
                "{}_{}_{}",
                stamp, settings.vendor, settings.name
            ))
            .join("log.md");
        eprintln!("live test log: {}", log_file.display());
        let logger: Arc<dyn Logger> =
            Arc::new(DoubleLogger::new(&log_file).expect("cannot open live test log"));
        let registry = Arc::new(ToolRegistry::build(
            &ToolsSettings::default(),
            ToolsContext {
                cwd: std::env::temp_dir(),
                temp_dir: std::env::temp_dir(),
                logger: logger.clone(),
            },
        ));
        (settings, logger, registry)
    }
}
