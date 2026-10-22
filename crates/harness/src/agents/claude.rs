//! Anthropic Claude agent over the Messages API.
//!
//! Conversation state is a local message list. Tools use Anthropic
//! `input_schema` (not OpenAI `function` / Gemini `functionDeclarations`).
//! USD is not reported by the API and stays `0.0`.

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tools::ToolRegistry;

use super::{claude_tools, invoke_tool, Agent, UsageStats};
use conf::{AgentKind, AgentSettings};

const CLAUDE_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: i64 = 8192;
/// Output cap for `claude-sonnet-5-5` (`max_tokens` is not the session token-limit).
const MODEL_MAX_OUTPUT_TOKENS: i64 = 128_000;

/// Claude agent (vendor `Claude`).
pub struct ClaudeAgent {
    tools: Arc<ToolRegistry>,
    logger: Arc<dyn Logger>,
    api_key: String,
    vendor: String,
    display_name: String,
    api_model: String,
    token_limit: i64,
    usd_limit: f64,
    usage: UsageStats,
    messages: Vec<Value>,
    client: reqwest::blocking::Client,
}

impl ClaudeAgent {
    /// Build an agent with a resolved API key (not a path, not a filename).
    pub fn new(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Claude { api_model } => api_model.clone(),
            _ => return Err("expected Claude agent settings".into()),
        };
        Ok(Self {
            tools,
            logger,
            api_key: agent.api_key.clone(),
            vendor: agent.vendor.clone(),
            display_name: agent.name.clone(),
            api_model,
            token_limit: agent.token_limit,
            usd_limit: agent.usd_limit,
            usage: UsageStats::default(),
            messages: Vec::new(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
        })
    }

    fn max_tokens(&self) -> i64 {
        claude_max_output_tokens(self.token_limit)
    }

    fn execute_request(&mut self) -> Result<ParsedClaude, String> {
        let tools = claude_tools(&self.tools.list());
        let body = json!({
            "model": self.api_model,
            "max_tokens": self.max_tokens(),
            "messages": self.messages,
            "tools": tools,
        });
        let response = self
            .client
            .post(CLAUDE_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| format!("Claude request failed: {e}"))?;
        let status = response.status();
        let text = response
            .text()
            .map_err(|e| format!("Claude read body failed: {e}"))?;
        if !status.is_success() {
            return Err(format!("Claude HTTP {status}: {text}"));
        }
        let parsed = parse_claude_response(&text)?;
        self.usage.requests += 1;
        self.usage.input_tokens += parsed.input_tokens;
        self.usage.output_tokens += parsed.output_tokens;
        Ok(parsed)
    }

    fn handle_tool_uses(&mut self, calls: &[ClaudeToolUse]) {
        self.logger.log_line("agent request tools:");
        let mut results = Vec::new();
        for (i, call) in calls.iter().enumerate() {
            let raw_args = call.input.to_string();
            let log_arg = if raw_args.len() > 150 {
                "...".to_string()
            } else {
                raw_args
            };
            self.logger
                .log_line(&format!("{}. {}({log_arg})", i + 1, call.name));
            let result = invoke_tool(&self.tools, &self.logger, &call.name, call.input.clone());
            if !result.starts_with("execution error:") {
                self.logger.log_line("✅ success");
            }
            results.push(json!({
                "type": "tool_result",
                "tool_use_id": call.id,
                "content": result,
            }));
        }
        self.messages.push(json!({
            "role": "user",
            "content": results,
        }));
    }
}

impl Agent for ClaudeAgent {
    fn vendor(&self) -> &str {
        &self.vendor
    }
    fn model(&self) -> &str {
        &self.display_name
    }
    fn tokens_limit(&self) -> i64 {
        self.token_limit
    }
    fn consumed_tokens(&self) -> i64 {
        self.usage.input_tokens + self.usage.output_tokens
    }
    fn limit_usd(&self) -> f64 {
        self.usd_limit
    }
    fn consumed_usd(&self) -> f64 {
        self.usage.total_cost_usd
    }

    fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String> {
        if let Some(prompt) = prompt {
            if !prompt.is_empty() {
                self.logger.log_line("user prompt:");
                self.logger.log_line("```");
                self.logger.log_line(prompt);
                self.logger.log_line("```");
                self.messages
                    .push(json!({"role": "user", "content": prompt}));
            }
        }
        let parsed = self.execute_request()?;
        if let Some(content) = &parsed.text {
            if !content.is_empty() {
                self.logger.log_line("agent content:");
                self.logger.log_line("```");
                self.logger.log_line(content);
                self.logger.log_line("```");
            }
        }
        self.messages.push(json!({
            "role": "assistant",
            "content": parsed.content,
        }));
        if parsed.tool_uses.is_empty() {
            Ok(true)
        } else {
            self.handle_tool_uses(&parsed.tool_uses);
            Ok(false)
        }
    }

    fn finish(&self) {
        let total = self.usage.input_tokens + self.usage.output_tokens;
        self.logger.log_line("Sessions statistic");
        self.logger
            .log_line(&format!("- total requests: {}", self.usage.requests));
        self.logger.log_line(&format!("- total tokens: {total}"));
        self.logger.log_line(&format!(
            "    → input tokens: {} (from stats)",
            self.usage.input_tokens
        ));
        self.logger.log_line(&format!(
            "    ← output tokens: {}",
            self.usage.output_tokens
        ));
        self.logger
            .log_line(&format!("- total cost: {}$", self.usage.total_cost_usd));
    }

    fn dump_session(&self, path: &Path) -> Result<(), String> {
        let data = ClaudeDump {
            messages: self.messages.clone(),
            usage: self.usage.clone(),
        };
        let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: ClaudeDump = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.messages = data.messages;
        self.usage = data.usage;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeDump {
    messages: Vec<Value>,
    usage: UsageStats,
}

#[derive(Debug, Clone)]
struct ClaudeToolUse {
    id: String,
    name: String,
    input: Value,
}

#[derive(Debug)]
struct ParsedClaude {
    text: Option<String>,
    tool_uses: Vec<ClaudeToolUse>,
    content: Value,
    input_tokens: i64,
    output_tokens: i64,
}

/// Parse an Anthropic Messages API body (unit-tested on fixtures).
fn parse_claude_response(text: &str) -> Result<ParsedClaude, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Claude json: {e}"))?;
    let content = v
        .get("content")
        .cloned()
        .ok_or_else(|| "Claude response has no content".to_string())?;
    let mut texts = Vec::new();
    let mut tool_uses = Vec::new();
    if let Some(arr) = content.as_array() {
        for block in arr {
            let ty = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if ty == "text" {
                if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                    texts.push(t.to_string());
                }
            } else if ty == "tool_use" {
                let id = block
                    .get("id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(json!({}));
                if !name.is_empty() {
                    tool_uses.push(ClaudeToolUse { id, name, input });
                }
            }
        }
    }
    let text_out = if texts.is_empty() {
        None
    } else {
        Some(texts.join(""))
    };
    let usage = v.get("usage").cloned().unwrap_or(Value::Null);
    Ok(ParsedClaude {
        text: text_out,
        tool_uses,
        content,
        input_tokens: usage
            .get("input_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        output_tokens: usage
            .get("output_tokens")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
    })
}

/// Anthropic `max_tokens` is an output cap, not the session token budget.
fn claude_max_output_tokens(token_limit: i64) -> i64 {
    let requested = if token_limit > 0 {
        token_limit
    } else {
        DEFAULT_MAX_TOKENS
    };
    requested.min(MODEL_MAX_OUTPUT_TOKENS)
}
