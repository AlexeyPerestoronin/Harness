//! xAI Grok agent over OpenAI-compatible `chat/completions`.
//!
//! Thin wrapper over [`OpenAiCompatAgent`] adding xAI-specific headers
//! (`x-grok-conv-id`), body field (`conversation_id`), cached-token mapping
//! (`cached_prompt_text_tokens`) and session ids in dump.

use std::path::Path;
use std::sync::Arc;

use conf::{AgentKind, AgentSettings};
use interactions::Interaction;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tools::ToolRegistry;
use uuid::Uuid;

use super::openai_compat::{OpenAiCompatAgent, OpenAiCompatConfig};
use super::Agent;

const XAI_URL: &str = "https://api.x.ai/v1/chat/completions";

/// Grok agent (vendor `SpaceXAI`).
pub struct XaiAgent {
    inner: OpenAiCompatAgent,
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
        let vendor_session = json!({
            "conv_id": Uuid::new_v4().to_string(),
            "chat_id": Uuid::new_v4().to_string(),
        });
        Ok(Self {
            inner: OpenAiCompatAgent::create_with(
                tools,
                interaction,
                agent,
                api_model,
                OpenAiCompatConfig {
                    url: XAI_URL,
                    vendor_label: "xAI",
                    cached_tokens: xai_cached_tokens,
                    vendor_session: Some(vendor_session),
                    vendor_headers: xai_headers,
                    vendor_body_fields: xai_body_fields,
                },
            ),
        })
    }
}

#[async_trait::async_trait]
impl Agent for XaiAgent {
    fn vendor(&self) -> &str {
        self.inner.vendor()
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn tokens_limit(&self) -> i64 {
        self.inner.tokens_limit()
    }
    fn consumed_tokens(&self) -> i64 {
        self.inner.consumed_tokens()
    }
    fn limit_usd(&self) -> f64 {
        self.inner.limit_usd()
    }
    fn consumed_usd(&self) -> f64 {
        self.inner.consumed_usd()
    }
    fn consumed_usd_real(&self) -> Option<f64> {
        self.inner.consumed_usd_real()
    }
    async fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String> {
        self.inner.iteration(prompt).await
    }
    async fn finish(&self) {
        self.inner.finish().await
    }
    fn dump_session(&self, path: &Path) -> Result<(), String> {
        self.inner.dump_session(path)
    }
    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        self.inner.reload_session(path)
    }
    fn set_cancel_token(&mut self, cancel: CancellationToken) {
        self.inner.set_cancel_token(cancel);
    }
}

fn xai_headers(session: &Value) -> Vec<(String, String)> {
    let conv_id = session
        .get("conv_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    vec![("x-grok-conv-id".to_string(), conv_id.to_string())]
}

fn xai_body_fields(session: &Value) -> Value {
    json!({
        "conversation_id": session.get("chat_id").cloned().unwrap_or(Value::Null),
    })
}

fn xai_cached_tokens(usage: &Value) -> i64 {
    if let Some(n) = usage
        .get("cached_prompt_text_tokens")
        .and_then(Value::as_i64)
    {
        return n;
    }
    usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}
