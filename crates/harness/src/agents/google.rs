//! Google Gemini agent over `generateContent` REST.
//!
//! Automatic function calling is off: the loop executes tools and sends
//! `functionResponse` parts back. HTTP 429 retries immediately (after the
//! rate limiter); HTTP 503 sleeps a random 5..=20 seconds. Up to 24 attempts.

use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use logger::Logger;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tools::ToolRegistry;

use conf::{AgentKind, AgentSettings};
use super::{gemini_tools, invoke_tool, Agent, UsageStats};

/// Gemini agent (vendor `GoogleAI`).
pub struct GoogleAgent {
    tools: Arc<ToolRegistry>,
    logger: Arc<dyn Logger>,
    api_key: String,
    vendor: String,
    display_name: String,
    api_model: String,
    token_limit: i64,
    usd_limit: f64,
    rpm: u32,
    last_call: Option<Instant>,
    usage: UsageStats,
    history: Vec<Value>,
    pending: Option<Value>,
    client: reqwest::blocking::Client,
}

impl GoogleAgent {
    /// Build an agent with a resolved API key and rpm from the model file.
    pub fn new(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Google { api_model } => api_model.clone(),
            _ => return Err("expected Google agent settings".into()),
        };
        let rpm = agent.rpm.ok_or_else(|| {
            format!("model '{}' requires '{}' in the model conf file", agent.name, conf::str::keys::RPM)
        })?;
        Ok(Self {
            tools,
            logger,
            api_key: agent.api_key.clone(),
            vendor: agent.vendor.clone(),
            display_name: agent.name.clone(),
            api_model,
            token_limit: agent.token_limit,
            usd_limit: agent.usd_limit,
            rpm,
            last_call: None,
            usage: UsageStats::default(),
            history: Vec::new(),
            pending: None,
            client: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("reqwest client"),
        })
    }

    fn wait_rate_limit(&mut self) {
        if self.rpm == 0 {
            return;
        }
        let min_interval = 60.0 / f64::from(self.rpm);
        if let Some(last) = self.last_call {
            let elapsed = last.elapsed().as_secs_f64();
            if elapsed < min_interval {
                let extra = min_interval - elapsed + 1.0;
                thread::sleep(Duration::from_secs_f64(extra.max(0.0)));
            }
        }
        self.last_call = Some(Instant::now());
    }

    fn url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.api_model
        )
    }

    fn execute_request(&mut self, message: &Value) -> Result<ParsedGoogle, String> {
        let mut contents = self.history.clone();
        contents.push(message.clone());
        let body = json!({
            "contents": contents,
            "tools": gemini_tools(&self.tools.list()),
        });
        let mut last_err = "Request failed after max retries".to_string();
        for _attempt in 1..=24 {
            self.wait_rate_limit();
            let response = self
                .client
                .post(self.url())
                .header("x-goog-api-key", &self.api_key)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .map_err(|e| format!("Google request failed: {e}"))?;
            let status = response.status();
            let text = response
                .text()
                .map_err(|e| format!("Google read body failed: {e}"))?;
            if status.as_u16() == 429 {
                self.logger.log_str(" fail 429: model limit exceeded!");
                last_err = "fail 429: model limit exceeded!".into();
                continue;
            }
            if status.as_u16() == 503 {
                let delay = rand::thread_rng().gen_range(5..=20);
                self.logger
                    .log_str(&format!(" fail 503: waiting {delay}s..."));
                thread::sleep(Duration::from_secs(delay));
                last_err = "fail 503".into();
                continue;
            }
            if !status.is_success() {
                return Err(format!("Google HTTP {status}: {text}"));
            }
            let parsed = parse_google_response(&text)?;
            self.usage.requests += 1;
            self.usage.input_tokens += parsed.prompt_tokens;
            self.usage.output_tokens += parsed.candidates_tokens;
            self.usage.cached_tokens += parsed.cached_tokens;
            self.history.push(message.clone());
            if let Some(model_content) = &parsed.model_content {
                self.history.push(model_content.clone());
            }
            return Ok(parsed);
        }
        Err(last_err)
    }

    fn handle_tool_calls(&self, calls: &[GoogleFnCall]) -> Value {
        self.logger.log_line("agent request tools:");
        let mut parts = Vec::new();
        for (i, fc) in calls.iter().enumerate() {
            let log_arg = if fc.name == "write_file" {
                "...".to_string()
            } else {
                fc.args.to_string()
            };
            self.logger
                .log_line(&format!("{}. {}({log_arg})", i + 1, fc.name));
            let result = invoke_tool(&self.tools, &self.logger, &fc.name, fc.args.clone());
            if result.starts_with("execution error:") {
                self.logger.log_str(&format!(" → fail → {result}"));
            } else {
                self.logger.log_str(" → success");
                if fc.name == "read_file" || fc.name == "write_file" {
                    self.logger
                        .log_str(&format!(" → read/write {}symbols", result.len()));
                } else {
                    self.logger.log_line("```");
                    self.logger.log_line(&result);
                    self.logger.log_line("```");
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

    fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String> {
        if let Some(prompt) = prompt {
            if !prompt.is_empty() {
                self.logger.log_line("user prompt:");
                self.logger.log_line("```");
                self.logger.log_line(prompt);
                self.logger.log_line("```");
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
        let parsed = self.execute_request(&message)?;
        if let Some(text) = &parsed.text {
            if !text.is_empty() {
                self.logger.log_line("agent content:");
                self.logger.log_line("```");
                self.logger.log_line(text);
                self.logger.log_line("```");
            }
        }
        if parsed.function_calls.is_empty() {
            Ok(true)
        } else {
            self.pending = Some(self.handle_tool_calls(&parsed.function_calls));
            Ok(false)
        }
    }

    fn finish(&self) {
        self.logger.log_line("Sessions statistic");
        self.logger
            .log_line(&format!("- total requests: {}", self.usage.requests));
        self.logger
            .log_line(&format!("- total tokens: {}", self.consumed_tokens()));
        self.logger.log_line(&format!(
            "    - input: {} (cached: {})",
            self.usage.input_tokens, self.usage.cached_tokens
        ));
        self.logger
            .log_line(&format!("    - output: {}", self.usage.output_tokens));
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_and_function_call() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        { "text": "hello" },
                        { "functionCall": { "name": "ask_user", "args": { "question": "q" } } }
                    ]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 7,
                "candidatesTokenCount": 3,
                "cachedContentTokenCount": 1
            }
        }"#;
        let p = parse_google_response(body).unwrap();
        assert_eq!(p.text.as_deref(), Some("hello"));
        assert_eq!(p.function_calls.len(), 1);
        assert_eq!(p.function_calls[0].name, "ask_user");
        assert_eq!(p.prompt_tokens, 7);
        assert_eq!(p.candidates_tokens, 3);
        assert_eq!(p.cached_tokens, 1);
    }

    #[test]
    fn dump_roundtrip() {
        let dump = GoogleDump {
            usage_stats: UsageStats {
                requests: 2,
                input_tokens: 5,
                output_tokens: 6,
                cached_tokens: 0,
                total_cost_usd: 0.0,
            },
            history: vec![json!({"role":"user","parts":[{"text":"ping"}]})],
            pending_message: None,
        };
        let text = serde_json::to_string(&dump).unwrap();
        let back: GoogleDump = serde_json::from_str(&text).unwrap();
        assert_eq!(back.usage_stats.requests, 2);
        assert_eq!(back.history.len(), 1);
    }

    #[test]
    #[ignore = "live API; cargo test -p harness -- --ignored --nocapture --test-threads=1"]
    fn live_availability() {
        let key = super::super::live_support::read_api_key("GoogleAI");
        let (logger, tools) = super::super::live_support::tools();
        let settings = super::super::live_support::settings(
            "GoogleAI",
            "Gemini 3.8 Flash",
            key,
            AgentKind::Google {
                api_model: "gemini-3.8-flash".into(),
            },
            Some(5),
        );
        let mut agent = GoogleAgent::new(tools, logger, &settings).unwrap();
        let result = agent.iteration(Some(super::super::live_support::PROMPT));
        assert!(
            result.is_ok(),
            "Google live request failed: {:?}",
            result.err()
        );
        assert!(agent.consumed_tokens() > 0);
    }
}
