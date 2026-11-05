//! Shared OpenAI-compatible `chat/completions` client for DeepSeek, AliBaba and xAI.
//!
//! Vendor files supply URL, `AgentKind`, API model and optional extensions:
//! extra headers, extra body fields, cache-token mapping and vendor session
//! data stored in dump files.

use std::path::Path;
use std::sync::Arc;

use conf::AgentSettings;
use interactions::Interaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tools::ToolRegistry;

use super::{
    accumulate_real_cost, approximate_usd, invoke_tool, log_line, log_usage_summary, openai_tools,
    real_cost_from_body, Agent, RealCost, UsageStats,
};
use crate::vendor_request::VendorRequest;

/// Session dump shared by DeepSeek and AliBaba (no vendor conversation id).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ChatDump {
    pub messages: Vec<Value>,
    pub usage_stats: UsageStats,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_session: Option<Value>,
}

/// One OpenAI-style `tool_calls[]` entry.
#[derive(Debug, Clone)]
pub(crate) struct ChatToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Vendor-specific extensions for an OpenAI-compatible agent.
pub(crate) struct OpenAiCompatConfig {
    pub url: &'static str,
    pub vendor_label: &'static str,
    pub cached_tokens: fn(&Value) -> i64,
    pub vendor_session: Option<Value>,
    pub vendor_headers: fn(&Value) -> Vec<(String, String)>,
    pub vendor_body_fields: fn(&Value) -> Value,
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
    pub real_cost: Option<RealCost>,
}

/// OpenAI-compatible chat agent used by DeepSeek, AliBaba and xAI wrappers.
pub(crate) struct OpenAiCompatAgent {
    tools: Arc<ToolRegistry>,
    interaction: Arc<dyn Interaction>,
    api_key: String,
    vendor: String,
    vendor_label: &'static str,
    display_name: String,
    api_model: String,
    url: &'static str,
    token_limit: i64,
    usd_limit: f64,
    input_price: f64,
    output_price: f64,
    cached_price: f64,
    usage: UsageStats,
    messages: Vec<Value>,
    http: VendorRequest,
    cancel: CancellationToken,
    cached_tokens: fn(&Value) -> i64,
    vendor_session: Option<Value>,
    vendor_headers: fn(&Value) -> Vec<(String, String)>,
    vendor_body_fields: fn(&Value) -> Value,
}

impl OpenAiCompatAgent {
    /// Build from already-resolved settings plus the vendor URL. Prices come
    /// from `AgentSettings`, not from a vendor-specific hardcoded table.
    pub(crate) fn create(
        tools: Arc<ToolRegistry>,
        interaction: Arc<dyn Interaction>,
        agent: &AgentSettings,
        api_model: String,
        url: &'static str,
        vendor_label: &'static str,
    ) -> Self {
        Self::create_with(
            tools,
            interaction,
            agent,
            api_model,
            OpenAiCompatConfig {
                url,
                vendor_label,
                cached_tokens: cached_tokens_from_usage,
                vendor_session: None,
                vendor_headers: empty_vendor_headers,
                vendor_body_fields: empty_vendor_body_fields,
            },
        )
    }

    /// Build from resolved settings plus a full vendor extension config.
    pub(crate) fn create_with(
        tools: Arc<ToolRegistry>,
        interaction: Arc<dyn Interaction>,
        agent: &AgentSettings,
        api_model: String,
        config: OpenAiCompatConfig,
    ) -> Self {
        Self {
            tools,
            interaction: interaction.clone(),
            api_key: agent.api_key.clone(),
            vendor: agent.vendor.clone(),
            vendor_label: config.vendor_label,
            display_name: agent.name.clone(),
            api_model,
            url: config.url,
            token_limit: agent.token_limit,
            usd_limit: agent.usd_limit,
            input_price: agent.input_price,
            output_price: agent.output_price,
            cached_price: agent.cached_price,
            usage: UsageStats::default(),
            messages: Vec::new(),
            http: VendorRequest::new(agent.rpm, agent.tpm, agent.rpd, interaction),
            cancel: CancellationToken::new(),
            cached_tokens: config.cached_tokens,
            vendor_session: config.vendor_session,
            vendor_headers: config.vendor_headers,
            vendor_body_fields: config.vendor_body_fields,
        }
    }

    async fn execute_request(&mut self) -> Result<ParsedChat, String> {
        let tools = openai_tools(&self.tools.list());
        let mut body = serde_json::Map::new();
        body.insert("model".into(), Value::String(self.api_model.clone()));
        body.insert("messages".into(), Value::Array(self.messages.clone()));
        body.insert("tools".into(), tools);
        let session = self.vendor_session.clone().unwrap_or(Value::Null);
        let extra = (self.vendor_body_fields)(&session);
        if let Value::Object(extra) = extra {
            for (key, value) in extra {
                body.insert(key, value);
            }
        }
        let body = Value::Object(body);
        let auth = format!("Bearer {}", self.api_key);
        let mut headers: Vec<(&str, &str)> = vec![
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/json"),
        ];
        let vendor_headers = (self.vendor_headers)(&session);
        for (key, value) in &vendor_headers {
            headers.push((key.as_str(), value.as_str()));
        }
        let text = self
            .http
            .send_json(self.url, &headers, &body, &self.cancel)
            .await?;
        let parsed = parse_chat_completion(&text, self.vendor_label, self.cached_tokens)?;
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

    async fn handle_tool_calls(&mut self, calls: &[ChatToolCall]) {
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
        log_usage_summary(
            &self.interaction,
            &self.usage,
            self.usage.input_tokens + self.usage.output_tokens,
        )
        .await;
    }

    fn dump_session(&self, path: &Path) -> Result<(), String> {
        let data = ChatDump {
            messages: self.messages.clone(),
            usage_stats: self.usage.clone(),
            vendor_session: self.vendor_session.clone(),
        };
        let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: ChatDump = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.messages = data.messages;
        self.usage = data.usage_stats;
        self.vendor_session = data.vendor_session;
        Ok(())
    }

    fn set_cancel_token(&mut self, cancel: CancellationToken) {
        self.cancel = cancel;
    }
}

/// Parse an OpenAI-compatible chat completion JSON body.
pub(crate) fn parse_chat_completion(
    text: &str,
    vendor: &str,
    cached_tokens: fn(&Value) -> i64,
) -> Result<ParsedChat, String> {
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
    let real_cost = real_cost_from_body(&v, &usage);
    Ok(ParsedChat {
        role,
        content,
        reasoning,
        tool_calls,
        message,
        prompt_tokens: json_i64(&usage, "prompt_tokens"),
        completion_tokens: json_i64(&usage, "completion_tokens"),
        cached_tokens: cached_tokens(&usage),
        real_cost,
    })
}

/// Map vendor cache-hit fields onto `UsageStats.cached_tokens`.
///
/// DeepSeek: `prompt_cache_hit_tokens` (also `prompt_tokens_details.cached_tokens`).
/// Qwen/DashScope: `prompt_tokens_details.cached_tokens` or
/// `input_tokens_details.cached_tokens`. Missing fields → `0`, never an error.
pub(crate) fn cached_tokens_from_usage(usage: &Value) -> i64 {
    if let Some(n) = usage
        .get("prompt_cache_hit_tokens")
        .and_then(|x| x.as_i64())
    {
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

fn json_i64(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(|x| x.as_i64()).unwrap_or(0)
}

fn empty_vendor_headers(_: &Value) -> Vec<(String, String)> {
    Vec::new()
}

fn empty_vendor_body_fields(_: &Value) -> Value {
    Value::Object(serde_json::Map::new())
}
