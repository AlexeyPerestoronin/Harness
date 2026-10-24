//! Shared OpenAI-compatible `chat/completions` client for DeepSeek and AliBaba.
//!
//! xAI keeps its own parser and headers (`x-grok-conv-id`, `cost_in_usd_ticks`).
//! This module owns the protocol both new vendors actually share: messages,
//! tools, cache-token mapping and USD arithmetic. Vendor files only supply
//! URL, `AgentKind` and per-model prices.

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tools::ToolRegistry;

use super::{invoke_tool, openai_tools, Agent, UsageStats};
use crate::vendor_request::VendorRequest;
use conf::AgentSettings;

/// USD prices per 1M tokens. `cache_hit` is `0.0` when the vendor has no
/// published cache-hit price — do not invent a discount coefficient.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TokenPrices {
    pub input: f64,
    pub cache_hit: f64,
    pub output: f64,
}

/// Session dump shared by DeepSeek and AliBaba (no vendor conversation id).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ChatDump {
    pub messages: Vec<Value>,
    pub usage_stats: UsageStats,
}

/// One OpenAI-style `tool_calls[]` entry.
#[derive(Debug, Clone)]
pub(crate) struct ChatToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Parsed `chat/completions` body (unit-tested on fixtures, no HTTP).
#[derive(Debug)]
pub(crate) struct ParsedChat {
    pub role: Option<String>,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ChatToolCall>,
    pub message: Value,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cached_tokens: i64,
}

/// OpenAI-compatible chat agent used by DeepSeek and AliBaba wrappers.
pub(crate) struct OpenAiCompatAgent {
    tools: Arc<ToolRegistry>,
    logger: Arc<dyn Logger>,
    api_key: String,
    vendor: String,
    vendor_label: &'static str,
    display_name: String,
    api_model: String,
    url: &'static str,
    token_limit: i64,
    usd_limit: f64,
    prices: TokenPrices,
    usage: UsageStats,
    messages: Vec<Value>,
    http: VendorRequest,
}

impl OpenAiCompatAgent {
    /// Build from already-resolved settings plus vendor URL / prices.
    pub(crate) fn create(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
        api_model: String,
        url: &'static str,
        vendor_label: &'static str,
        prices: TokenPrices,
    ) -> Self {
        Self {
            tools,
            logger: logger.clone(),
            api_key: agent.api_key.clone(),
            vendor: agent.vendor.clone(),
            vendor_label,
            display_name: agent.name.clone(),
            api_model,
            url,
            token_limit: agent.token_limit,
            usd_limit: agent.usd_limit,
            prices,
            usage: UsageStats::default(),
            messages: Vec::new(),
            http: VendorRequest::new(agent.rpm, agent.tpm, agent.rpd, logger),
        }
    }

    fn execute_request(&mut self) -> Result<ParsedChat, String> {
        let tools = openai_tools(&self.tools.list());
        let body = json!({
            "model": self.api_model,
            "messages": self.messages,
            "tools": tools,
        });
        let auth = format!("Bearer {}", self.api_key);
        let text = self.http.send_json(
            self.url,
            &[
                ("Authorization", auth.as_str()),
                ("Content-Type", "application/json"),
            ],
            &body,
        )?;
        let parsed = parse_chat_completion(&text, self.vendor_label)?;
        self.http
            .record_tokens(parsed.prompt_tokens + parsed.completion_tokens);
        self.usage.requests += 1;
        self.usage.input_tokens += parsed.prompt_tokens;
        self.usage.output_tokens += parsed.completion_tokens;
        self.usage.cached_tokens += parsed.cached_tokens;
        self.usage.total_cost_usd += usd_cost(
            parsed.prompt_tokens,
            parsed.cached_tokens,
            parsed.completion_tokens,
            self.prices,
        );
        Ok(parsed)
    }

    fn handle_tool_calls(&mut self, calls: &[ChatToolCall]) {
        self.logger.log_line("agent request tools:");
        for (i, call) in calls.iter().enumerate() {
            let raw_args = if call.arguments.is_empty() {
                "{}".to_string()
            } else {
                call.arguments.clone()
            };
            let log_arg = if raw_args.len() > 150 {
                "...".to_string()
            } else {
                raw_args.clone()
            };
            self.logger
                .log_line(&format!("{}. {}({log_arg})", i + 1, call.name));
            let args: Value = serde_json::from_str(&raw_args).unwrap_or_else(|_| json!({}));
            let result = invoke_tool(&self.tools, &self.logger, &call.name, args);
            if !result.starts_with("execution error:") {
                self.logger.log_line("✅ success");
            }
            self.messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result,
            }));
        }
    }
}

impl Agent for OpenAiCompatAgent {
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
        self.usage.input_tokens + self.usage.output_tokens - self.usage.cached_tokens
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
        if let Some(role) = &parsed.role {
            self.logger.log_line(&format!("role: {role}"));
        }
        if let Some(reasoning) = &parsed.reasoning {
            if !reasoning.is_empty() {
                self.logger.log_line("request reasoning:");
                self.logger.log_line("```");
                self.logger.log_line(reasoning);
                self.logger.log_line("```");
            }
        }
        if let Some(content) = &parsed.content {
            if !content.is_empty() {
                self.logger.log_line("agent content:");
                self.logger.log_line("```");
                self.logger.log_line(content);
                self.logger.log_line("```");
            }
        }
        self.messages.push(parsed.message.clone());
        if parsed.tool_calls.is_empty() {
            Ok(true)
        } else {
            self.handle_tool_calls(&parsed.tool_calls);
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
        self.logger.log_line(&format!(
            "    ↔ cached tokens: {}",
            self.usage.cached_tokens
        ));
        self.logger
            .log_line(&format!("- total cost: {}$", self.usage.total_cost_usd));
    }

    fn dump_session(&self, path: &Path) -> Result<(), String> {
        let data = ChatDump {
            messages: self.messages.clone(),
            usage_stats: self.usage.clone(),
        };
        let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: ChatDump = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.messages = data.messages;
        self.usage = data.usage_stats;
        Ok(())
    }
}

/// Parse an OpenAI-compatible chat completion JSON body.
pub(crate) fn parse_chat_completion(text: &str, vendor: &str) -> Result<ParsedChat, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("{vendor} json: {e}"))?;
    let choice = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| format!("{vendor} response has no choices"))?;
    let message = choice
        .get("message")
        .cloned()
        .ok_or_else(|| format!("{vendor} choice has no message"))?;
    let role = message
        .get("role")
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());
    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());
    let reasoning = message
        .get("reasoning_content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());
    let mut tool_calls = Vec::new();
    if let Some(arr) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in arr {
            let id = tc
                .get("id")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let func = tc.get("function").cloned().unwrap_or(Value::Null);
            let name = func
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = match func.get("arguments") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "{}".to_string(),
            };
            if !name.is_empty() {
                tool_calls.push(ChatToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }
    }
    let usage = v.get("usage").cloned().unwrap_or(Value::Null);
    Ok(ParsedChat {
        role,
        content,
        reasoning,
        tool_calls,
        message,
        prompt_tokens: json_i64(&usage, "prompt_tokens"),
        completion_tokens: json_i64(&usage, "completion_tokens"),
        cached_tokens: cached_tokens_from_usage(&usage),
    })
}

/// Map vendor cache-hit fields onto `UsageStats.cached_tokens`.
///
/// DeepSeek: `prompt_cache_hit_tokens` (also `prompt_tokens_details.cached_tokens`).
/// Qwen/DashScope: `prompt_tokens_details.cached_tokens` or
/// `input_tokens_details.cached_tokens`. Missing fields → `0`, never an error.
pub(crate) fn cached_tokens_from_usage(usage: &Value) -> i64 {
    if let Some(n) = usage.get("prompt_cache_hit_tokens").and_then(|x| x.as_i64()) {
        return n;
    }
    if let Some(n) = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|x| x.as_i64())
    {
        return n;
    }
    if let Some(n) = usage
        .get("input_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|x| x.as_i64())
    {
        return n;
    }
    0
}

/// `usd = uncached_input/1e6 * input + cached_input/1e6 * cache_hit + output/1e6 * output`.
///
/// `uncached_input = max(input_tokens - cached_tokens, 0)`. When `cache_hit` is
/// `0.0` the cache tokens are free rather than guessed at ~90% off.
pub(crate) fn usd_cost(
    input_tokens: i64,
    cached_tokens: i64,
    output_tokens: i64,
    prices: TokenPrices,
) -> f64 {
    let cached = cached_tokens.max(0);
    let uncached = (input_tokens - cached).max(0);
    (uncached as f64) / 1_000_000.0 * prices.input
        + (cached as f64) / 1_000_000.0 * prices.cache_hit
        + (output_tokens.max(0) as f64) / 1_000_000.0 * prices.output
}

fn json_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEEPSEEK_V3: TokenPrices = TokenPrices {
        input: 0.14,
        cache_hit: 0.014,
        output: 0.28,
    };

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "usd {actual} != {expected}"
        );
    }

    #[test]
    fn deepseek_cache_hit_tokens_and_usd() {
        let text = r#"{
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 50,
                "prompt_cache_hit_tokens": 400,
                "prompt_cache_miss_tokens": 600
            }
        }"#;
        let parsed = parse_chat_completion(text, "DeepSeek").unwrap();
        assert_eq!(parsed.cached_tokens, 400);
        assert_eq!(parsed.prompt_tokens, 1000);
        assert_eq!(parsed.completion_tokens, 50);
        let usd = usd_cost(
            parsed.prompt_tokens,
            parsed.cached_tokens,
            parsed.completion_tokens,
            DEEPSEEK_V3,
        );
        // uncached 600 * 0.14 + cached 400 * 0.014 + output 50 * 0.28, per 1M
        assert_close(
            usd,
            600.0 / 1e6 * 0.14 + 400.0 / 1e6 * 0.014 + 50.0 / 1e6 * 0.28,
        );
    }

    #[test]
    fn deepseek_without_cache_fields_is_full_input_price() {
        let text = r#"{
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "usage": {
                "prompt_tokens": 1000,
                "completion_tokens": 50
            }
        }"#;
        let parsed = parse_chat_completion(text, "DeepSeek").unwrap();
        assert_eq!(parsed.cached_tokens, 0);
        let usd = usd_cost(
            parsed.prompt_tokens,
            parsed.cached_tokens,
            parsed.completion_tokens,
            DEEPSEEK_V3,
        );
        assert_close(usd, 1000.0 / 1e6 * 0.14 + 50.0 / 1e6 * 0.28);
    }

    #[test]
    fn qwen_max_tool_calls() {
        let text = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"a.rs\"}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "prompt_tokens_details": {"cached_tokens": 40}
            }
        }"#;
        let parsed = parse_chat_completion(text, "AliBaba").unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call_1");
        assert_eq!(parsed.tool_calls[0].name, "read_file");
        assert_eq!(parsed.tool_calls[0].arguments, r#"{"path":"a.rs"}"#);
        assert_eq!(parsed.cached_tokens, 40);
    }

    #[test]
    fn deepseek_r1_reasoning_content_is_extracted() {
        let text = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "answer",
                    "reasoning_content": "think step by step"
                }
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }"#;
        let parsed = parse_chat_completion(text, "DeepSeek").unwrap();
        assert_eq!(parsed.reasoning.as_deref(), Some("think step by step"));
        assert_eq!(parsed.content.as_deref(), Some("answer"));
    }
}
