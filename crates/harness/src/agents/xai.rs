//! xAI Grok agent over OpenAI-compatible `chat/completions`.
//!
//! Conversation state is a local message list. The `x-grok-conv-id` header is
//! sent on every request. Token accounting: `input + output - cached`.

use std::sync::Arc;
use std::time::Duration;
use std::{path::Path, thread};

use logger::Logger;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tools::ToolRegistry;
use uuid::Uuid;

use super::{invoke_tool, openai_tools, Agent, UsageStats};
use conf::{AgentKind, AgentSettings};

const XAI_URL: &str = "https://api.x.ai/v1/chat/completions";

/// Grok agent (vendor `SpaceXAI`).
pub struct XaiAgent {
    tools: Arc<ToolRegistry>,
    logger: Arc<dyn Logger>,
    api_key: String,
    vendor: String,
    display_name: String,
    api_model: String,
    token_limit: i64,
    usd_limit: f64,
    conv_id: String,
    chat_id: String,
    usage: UsageStats,
    messages: Vec<Value>,
    client: reqwest::blocking::Client,
}

impl XaiAgent {
    /// Build an agent with a resolved API key (not a path, not a filename).
    pub fn new(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Xai { api_model } => api_model.clone(),
            _ => return Err("expected xAI agent settings".into()),
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
            conv_id: Uuid::new_v4().to_string(),
            chat_id: Uuid::new_v4().to_string(),
            usage: UsageStats::default(),
            messages: Vec::new(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
        })
    }

    fn wait_rate_limit(&mut self) {
        // TODO: need to implement RPM limitation for xAI
        thread::sleep(Duration::from_secs_f64(15f64));
    }

    fn execute_request(&mut self) -> Result<ParsedXai, String> {
        let tools = openai_tools(&self.tools.list());
        let body = json!({
            "model": self.api_model,
            "messages": self.messages,
            "tools": tools,
            "conversation_id": self.chat_id,
        });
        let mut last_err = "Request failed after max retries".to_string();
        for attempt in 1..=24 {
            let response = self
                .client
                .post(XAI_URL)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("x-grok-conv-id", &self.conv_id)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("⚠️ {attempt}-attempt of xAI request failed: {e}"));
            if let Err(error) = &response {
                last_err = error.clone();
                self.logger.log_line(error.as_str());
                self.wait_rate_limit();
                continue;
            }
            let response = response?;
            let status = response.status();
            let text = response
                .text()
                .map_err(|e| format!("xAI read body failed: {e}"))?;
            if !status.is_success() {
                return Err(format!("xAI HTTP {status}: {text}"));
            }
            let parsed = parse_xai_response(&text)?;
            self.usage.requests += 1;
            self.usage.input_tokens += parsed.prompt_tokens;
            self.usage.output_tokens += parsed.completion_tokens;
            self.usage.cached_tokens += parsed.cached_tokens;
            self.usage.total_cost_usd += parsed.cost_usd;
            return Ok(parsed);
        }
        self.logger.log_line("❌ xAI requesting was failed!");
        Err(last_err)
    }

    fn handle_tool_calls(&mut self, calls: &[XaiToolCall]) {
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

impl Agent for XaiAgent {
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
        let data = XaiDump {
            conv_id: self.conv_id.clone(),
            chat_id: self.chat_id.clone(),
            usage_stats: self.usage.clone(),
            messages: self.messages.clone(),
        };
        let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: XaiDump = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.conv_id = data.conv_id;
        self.chat_id = data.chat_id;
        self.usage = data.usage_stats;
        self.messages = data.messages;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct XaiDump {
    conv_id: String,
    chat_id: String,
    usage_stats: UsageStats,
    messages: Vec<Value>,
}

#[derive(Debug, Clone)]
struct XaiToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug)]
struct ParsedXai {
    role: Option<String>,
    content: Option<String>,
    reasoning: Option<String>,
    tool_calls: Vec<XaiToolCall>,
    message: Value,
    prompt_tokens: i64,
    completion_tokens: i64,
    cached_tokens: i64,
    cost_usd: f64,
}

/// Parse an OpenAI-compatible chat completion body (unit-tested on fixtures).
fn parse_xai_response(text: &str) -> Result<ParsedXai, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("xAI json: {e}"))?;
    let choice = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| "xAI response has no choices".to_string())?;
    let message = choice
        .get("message")
        .cloned()
        .ok_or_else(|| "xAI choice has no message".to_string())?;
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
            let arguments = func
                .get("arguments")
                .and_then(|x| x.as_str())
                .unwrap_or("{}")
                .to_string();
            if !name.is_empty() {
                tool_calls.push(XaiToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }
    }
    let usage = v.get("usage").cloned().unwrap_or(Value::Null);
    let prompt_tokens = json_i64(&usage, "prompt_tokens");
    let completion_tokens = json_i64(&usage, "completion_tokens");
    let cached_tokens = usage
        .get("cached_prompt_text_tokens")
        .and_then(|x| x.as_i64())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|x| x.as_i64())
        })
        .unwrap_or(0);
    let cost_usd = parse_cost_usd(&v, &usage);
    Ok(ParsedXai {
        role,
        content,
        reasoning,
        tool_calls,
        message,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        cost_usd,
    })
}

fn json_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

fn json_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_u64().map(|n| n as f64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// xAI puts USD on `usage` (and sometimes on the root). Values may be numbers or numeric strings.
fn parse_cost_usd(root: &Value, usage: &Value) -> f64 {
    // 1. Сначала ищем точные тики (целое число) в новых ответах xAI
    for src in [usage, root] {
        if let Some(ticks) = src.get("cost_in_usd_ticks").and_then(|v| v.as_u64()) {
            const TICKS_PER_USD: f64 = 10_000_000_000.0;
            return ticks as f64 / TICKS_PER_USD;
        }
    }

    // 2. Если тиков нет, откатываемся к старым ключам с плавающей запятой
    const KEYS: &[&str] = &[
        "cost_in_usd",
        "cost_usd",
        "cost",
        "total_cost_usd",
        "total_cost",
    ];
    for src in [usage, root] {
        for key in KEYS {
            if let Some(v) = src.get(*key).and_then(json_f64) {
                return v;
            }
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_and_tool_call() {
        let body = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "hi",
                    "reasoning_content": "think",
                    "tool_calls": [{
                        "id": "c1",
                        "type": "function",
                        "function": { "name": "read_file", "arguments": "{\"path\":\"a.txt\"}" }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "cached_prompt_text_tokens": 2,
                "cost": 0.01
            }
        }"#;
        let p = parse_xai_response(body).unwrap();
        assert_eq!(p.content.as_deref(), Some("hi"));
        assert_eq!(p.reasoning.as_deref(), Some("think"));
        assert_eq!(p.tool_calls.len(), 1);
        assert_eq!(p.tool_calls[0].name, "read_file");
        assert_eq!(p.prompt_tokens, 10);
        assert_eq!(p.completion_tokens, 4);
        assert_eq!(p.cached_tokens, 2);
        assert!((p.cost_usd - 0.01).abs() < f64::EPSILON);
    }

    #[test]
    fn dump_roundtrip_shape() {
        let dump = XaiDump {
            conv_id: "cv".into(),
            chat_id: "ch".into(),
            usage_stats: UsageStats {
                requests: 1,
                input_tokens: 3,
                output_tokens: 2,
                cached_tokens: 1,
                total_cost_usd: 0.0,
            },
            messages: vec![json!({"role":"user","content":"ping"})],
        };
        let text = serde_json::to_string(&dump).unwrap();
        let back: XaiDump = serde_json::from_str(&text).unwrap();
        assert_eq!(back.conv_id, "cv");
        assert_eq!(back.usage_stats.input_tokens, 3);
        assert_eq!(back.messages.len(), 1);
    }

    #[test]
    fn parse_cost_from_numeric_string_and_cost_in_usd() {
        let body = r#"{
            "choices": [{ "message": { "role": "assistant", "content": "hi" } }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "cost_in_usd": "0.02"
            }
        }"#;
        let p = parse_xai_response(body).unwrap();
        assert!((p.cost_usd - 0.02).abs() < 1e-9);
    }

    #[test]
    fn parse_cost_from_root_cost() {
        let body = r#"{
            "choices": [{ "message": { "role": "assistant", "content": "hi" } }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 },
            "cost": 0.03
        }"#;
        let p = parse_xai_response(body).unwrap();
        assert!((p.cost_usd - 0.03).abs() < 1e-9);
    }
}
