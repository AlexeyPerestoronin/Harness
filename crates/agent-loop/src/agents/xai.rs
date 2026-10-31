//! xAI Grok agent over OpenAI-compatible `chat/completions`.
//!
//! Conversation state is a local message list. The `x-grok-conv-id` header is
//! sent on every request. Token accounting: `input + output - cached`.
//! USD approximation uses model prices from settings; real cost is parsed
//! from response bodies when present.
//! Rate limits and retries go through [`crate::vendor_request::VendorRequest`].

use std::path::Path;
use std::sync::Arc;

use conf::{AgentKind, AgentSettings};
use interactions::Interaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tools::ToolRegistry;
use uuid::Uuid;

use super::{
    accumulate_real_cost, approximate_usd, format_real_cost, invoke_tool, log_line, openai_tools,
    real_cost_from_body, Agent, RealCost, UsageStats,
};
use crate::vendor_request::VendorRequest;

const XAI_URL: &str = "https://api.x.ai/v1/chat/completions";

/// Grok agent (vendor `SpaceXAI`).
pub struct XaiAgent {
    tools: Arc<ToolRegistry>,
    interaction: Arc<dyn Interaction>,
    api_key: String,
    vendor: String,
    display_name: String,
    api_model: String,
    token_limit: i64,
    usd_limit: f64,
    input_price: f64,
    output_price: f64,
    cached_price: f64,
    conv_id: String,
    chat_id: String,
    usage: UsageStats,
    messages: Vec<Value>,
    http: VendorRequest,
    cancel: CancellationToken,
}

impl XaiAgent {
    /// Build an agent with a resolved API key (not a path, not a filename).
    pub fn new(
        tools: Arc<ToolRegistry>,
        interaction: Arc<dyn Interaction>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Xai { api_model } => api_model.clone(),
            _ => return Err("expected xAI agent settings".into()),
        };
        Ok(Self {
            tools,
            interaction: interaction.clone(),
            api_key: agent.api_key.clone(),
            vendor: agent.vendor.clone(),
            display_name: agent.name.clone(),
            api_model,
            token_limit: agent.token_limit,
            usd_limit: agent.usd_limit,
            input_price: agent.input_price,
            output_price: agent.output_price,
            cached_price: agent.cached_price,
            conv_id: Uuid::new_v4().to_string(),
            chat_id: Uuid::new_v4().to_string(),
            usage: UsageStats::default(),
            messages: Vec::new(),
            http: VendorRequest::new(agent.rpm, agent.tpm, agent.rpd, interaction),
            cancel: CancellationToken::new(),
        })
    }

    async fn execute_request(&mut self) -> Result<ParsedXai, String> {
        let tools = openai_tools(&self.tools.list());
        let body = json!({
            "model": self.api_model,
            "messages": self.messages,
            "tools": tools,
            "conversation_id": self.chat_id,
        });
        let auth = format!("Bearer {}", self.api_key);
        let conv_id = self.conv_id.clone();
        let text = self
            .http
            .send_json(
                XAI_URL,
                &[
                    ("Authorization", auth.as_str()),
                    ("x-grok-conv-id", conv_id.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &body,
                &self.cancel,
            )
            .await?;
        let parsed = parse_xai_response(&text)?;
        self.http
            .record_tokens(parsed.prompt_tokens + parsed.completion_tokens);
        self.usage.requests += 1;
        self.usage.input_tokens += parsed.prompt_tokens;
        self.usage.output_tokens += parsed.completion_tokens;
        self.usage.cached_tokens += parsed.cached_tokens;
        self.usage.total_cost_usd += approximate_usd(
            parsed.prompt_tokens,
            parsed.cached_tokens,
            parsed.completion_tokens,
            self.input_price,
            self.output_price,
            self.cached_price,
        );
        self.usage.real_cost_usd = accumulate_real_cost(self.usage.real_cost_usd, parsed.real_cost);
        Ok(parsed)
    }

    async fn handle_tool_calls(&mut self, calls: &[XaiToolCall]) {
        log_line(&self.interaction, "agent request tools:").await;
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
            log_line(
                &self.interaction,
                &format!("{}. {}({log_arg})", i + 1, call.name),
            )
            .await;
            let args: Value = serde_json::from_str(&raw_args).unwrap_or_else(|_| json!({}));
            let result = invoke_tool(&self.tools, &self.interaction, &call.name, args).await;
            if !result.starts_with("execution error:") {
                log_line(&self.interaction, "✅ success").await;
            }
            self.messages.push(json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": result,
            }));
        }
    }
}

#[async_trait::async_trait]
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
    fn consumed_usd_real(&self) -> Option<f64> {
        self.usage.real_cost_usd
    }

    async fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String> {
        if let Some(prompt) = prompt {
            if !prompt.is_empty() {
                log_line(&self.interaction, "user prompt:").await;
                log_line(&self.interaction, "```").await;
                log_line(&self.interaction, prompt).await;
                log_line(&self.interaction, "```").await;
                self.messages
                    .push(json!({"role": "user", "content": prompt}));
            }
        }
        let parsed = self.execute_request().await?;
        if let Some(role) = &parsed.role {
            log_line(&self.interaction, &format!("role: {role}")).await;
        }
        if let Some(reasoning) = &parsed.reasoning {
            if !reasoning.is_empty() {
                log_line(&self.interaction, "request reasoning:").await;
                log_line(&self.interaction, "```").await;
                log_line(&self.interaction, reasoning).await;
                log_line(&self.interaction, "```").await;
            }
        }
        if let Some(content) = &parsed.content {
            if !content.is_empty() {
                log_line(&self.interaction, "agent content:").await;
                log_line(&self.interaction, "```").await;
                log_line(&self.interaction, content).await;
                log_line(&self.interaction, "```").await;
            }
        }
        self.messages.push(parsed.message.clone());
        if parsed.tool_calls.is_empty() {
            Ok(true)
        } else {
            self.handle_tool_calls(&parsed.tool_calls).await;
            Ok(false)
        }
    }

    async fn finish(&self) {
        let total = self.usage.input_tokens + self.usage.output_tokens;
        log_line(&self.interaction, "Sessions statistic").await;
        log_line(
            &self.interaction,
            &format!("- total requests: {}", self.usage.requests),
        )
        .await;
        log_line(&self.interaction, &format!("- total tokens: {total}")).await;
        log_line(
            &self.interaction,
            &format!(
                "    → input tokens: {} (from stats)",
                self.usage.input_tokens
            ),
        )
        .await;
        log_line(
            &self.interaction,
            &format!("    ← output tokens: {}", self.usage.output_tokens),
        )
        .await;
        log_line(
            &self.interaction,
            &format!("    ↔ cached tokens: {}", self.usage.cached_tokens),
        )
        .await;
        log_line(
            &self.interaction,
            &format!(
                "- USD approximated: {}",
                super::format_usd(self.usage.total_cost_usd)
            ),
        )
        .await;
        log_line(
            &self.interaction,
            &format!(
                "- USD real: {}",
                format_real_cost(self.usage.real_cost_usd)
            ),
        )
        .await;
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

    fn set_cancel_token(&mut self, cancel: CancellationToken) {
        self.cancel = cancel;
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
    real_cost: Option<RealCost>,
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
    let real_cost = real_cost_from_body(&v, &usage);
    Ok(ParsedXai {
        role,
        content,
        reasoning,
        tool_calls,
        message,
        prompt_tokens,
        completion_tokens,
        cached_tokens,
        real_cost,
    })
}

fn json_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}
