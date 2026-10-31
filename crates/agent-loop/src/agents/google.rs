//! Google Gemini agent over `generateContent` REST.
//!
//! Automatic function calling is off: the loop executes tools and sends
//! `functionResponse` parts back. USD approximation uses model prices from
//! settings; real cost is parsed from response bodies when present.
//! Rate limits and retries go through
//! [`crate::vendor_request::VendorRequest`].

use std::path::Path;
use std::sync::Arc;

use conf::{AgentKind, AgentSettings};
use interactions::Interaction;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tools::ToolRegistry;

use super::{
    accumulate_real_cost, approximate_usd, format_real_cost, gemini_tools, invoke_tool, log_cont,
    log_line, real_cost_from_body, Agent, RealCost, UsageStats,
};
use crate::vendor_request::VendorRequest;

/// Gemini agent (vendor `GoogleAI`).
pub struct GoogleAgent {
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
    http: VendorRequest,
    usage: UsageStats,
    history: Vec<Value>,
    pending: Option<Value>,
    cancel: CancellationToken,
}

impl GoogleAgent {
    /// Build an agent with a resolved API key.
    pub fn new(
        tools: Arc<ToolRegistry>,
        interaction: Arc<dyn Interaction>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Google { api_model } => api_model.clone(),
            _ => return Err("expected Google agent settings".into()),
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
            http: VendorRequest::new(agent.rpm, agent.tpm, agent.rpd, interaction),
            usage: UsageStats::default(),
            history: Vec::new(),
            pending: None,
            cancel: CancellationToken::new(),
        })
    }

    fn url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.api_model
        )
    }

    async fn execute_request(&mut self, message: &Value) -> Result<ParsedGoogle, String> {
        let mut contents = self.history.clone();
        contents.push(message.clone());
        let body = json!({
            "contents": contents,
            "tools": gemini_tools(&self.tools.list()),
        });
        let url = self.url();
        let api_key = self.api_key.clone();
        let text = self
            .http
            .send_json(
                &url,
                &[
                    ("x-goog-api-key", api_key.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &body,
                &self.cancel,
            )
            .await?;
        let parsed = parse_google_response(&text)?;
        self.http
            .record_tokens(parsed.prompt_tokens + parsed.candidates_tokens);
        self.usage.requests += 1;
        self.usage.input_tokens += parsed.prompt_tokens;
        self.usage.output_tokens += parsed.candidates_tokens;
        self.usage.cached_tokens += parsed.cached_tokens;
        self.usage.total_cost_usd += approximate_usd(
            parsed.prompt_tokens,
            parsed.cached_tokens,
            parsed.candidates_tokens,
            self.input_price,
            self.output_price,
            self.cached_price,
        );
        self.usage.real_cost_usd = accumulate_real_cost(self.usage.real_cost_usd, parsed.real_cost);
        self.history.push(message.clone());
        if let Some(model_content) = &parsed.model_content {
            self.history.push(model_content.clone());
        }
        Ok(parsed)
    }

    async fn handle_tool_calls(&self, calls: &[GoogleFnCall]) -> Value {
        log_line(&self.interaction, "agent request tools:").await;
        let mut parts = Vec::new();
        for (i, fc) in calls.iter().enumerate() {
            let log_arg = if fc.name == "write_file" {
                "...".to_string()
            } else {
                fc.args.to_string()
            };
            log_line(
                &self.interaction,
                &format!("{}. {}({log_arg})", i + 1, fc.name),
            )
            .await;
            let result = invoke_tool(&self.tools, &self.interaction, &fc.name, fc.args.clone()).await;
            if result.starts_with("execution error:") {
                log_cont(&self.interaction, &format!(" → fail → {result}")).await;
            } else {
                log_cont(&self.interaction, " → success").await;
                if fc.name == "read_file" || fc.name == "write_file" {
                    log_cont(
                        &self.interaction,
                        &format!(" → read/write {}symbols", result.len()),
                    )
                    .await;
                } else {
                    log_line(&self.interaction, "```").await;
                    log_line(&self.interaction, &result).await;
                    log_line(&self.interaction, "```").await;
                }
            }
            parts.push(json!({
                "functionResponse": {
                    "name": fc.name,
                    "response": { "result": result }
                }
            }));
        }
        json!({ "role": "user", "parts": parts })
    }
}

#[async_trait::async_trait]
impl Agent for GoogleAgent {
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
                self.pending = Some(json!({
                    "role": "user",
                    "parts": [{ "text": prompt }]
                }));
            }
        }
        let message = self
            .pending
            .take()
            .ok_or_else(|| "Google agent has no pending message".to_string())?;
        let parsed = self.execute_request(&message).await?;
        if let Some(text) = &parsed.text {
            if !text.is_empty() {
                log_line(&self.interaction, "agent content:").await;
                log_line(&self.interaction, "```").await;
                log_line(&self.interaction, text).await;
                log_line(&self.interaction, "```").await;
            }
        }
        if parsed.function_calls.is_empty() {
            Ok(true)
        } else {
            self.pending = Some(self.handle_tool_calls(&parsed.function_calls).await);
            Ok(false)
        }
    }

    async fn finish(&self) {
        log_line(&self.interaction, "Sessions statistic").await;
        log_line(
            &self.interaction,
            &format!("- total requests: {}", self.usage.requests),
        )
        .await;
        log_line(
            &self.interaction,
            &format!("- total tokens: {}", self.consumed_tokens()),
        )
        .await;
        log_line(
            &self.interaction,
            &format!(
                "    - input: {} (cached: {})",
                self.usage.input_tokens, self.usage.cached_tokens
            ),
        )
        .await;
        log_line(
            &self.interaction,
            &format!("    - output: {}", self.usage.output_tokens),
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
        let data = GoogleDump {
            usage_stats: self.usage.clone(),
            history: self.history.clone(),
            pending_message: self.pending.clone(),
        };
        let text = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let data: GoogleDump = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.usage = data.usage_stats;
        self.history = data.history;
        self.pending = data.pending_message;
        Ok(())
    }

    fn set_cancel_token(&mut self, cancel: CancellationToken) {
        self.cancel = cancel;
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleDump {
    usage_stats: UsageStats,
    history: Vec<Value>,
    pending_message: Option<Value>,
}

#[derive(Debug, Clone)]
struct GoogleFnCall {
    name: String,
    args: Value,
}

#[derive(Debug)]
struct ParsedGoogle {
    text: Option<String>,
    function_calls: Vec<GoogleFnCall>,
    model_content: Option<Value>,
    prompt_tokens: i64,
    candidates_tokens: i64,
    cached_tokens: i64,
    real_cost: Option<RealCost>,
}

/// Parse a `generateContent` JSON body (unit-tested on fixtures).
fn parse_google_response(text: &str) -> Result<ParsedGoogle, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("Google json: {e}"))?;
    let candidate = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first());
    let model_content = candidate.and_then(|c| c.get("content")).cloned();
    let mut text_out = None;
    let mut function_calls = Vec::new();
    if let Some(parts) = model_content
        .as_ref()
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
    {
        let mut texts = Vec::new();
        for part in parts {
            if let Some(t) = part.get("text").and_then(|x| x.as_str()) {
                texts.push(t.to_string());
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                if !name.is_empty() {
                    let args = fc.get("args").cloned().unwrap_or(json!({}));
                    function_calls.push(GoogleFnCall { name, args });
                }
            }
        }
        if !texts.is_empty() {
            text_out = Some(texts.join(""));
        }
    }
    let usage = v.get("usageMetadata").cloned().unwrap_or(Value::Null);
    let real_cost = real_cost_from_body(&v, &usage);
    Ok(ParsedGoogle {
        text: text_out,
        function_calls,
        model_content,
        prompt_tokens: usage
            .get("promptTokenCount")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        candidates_tokens: usage
            .get("candidatesTokenCount")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        cached_tokens: usage
            .get("cachedContentTokenCount")
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        real_cost,
    })
}
