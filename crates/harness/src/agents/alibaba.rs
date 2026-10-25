//! AliBaba (Qwen) agent over DashScope OpenAI-compatible `chat/completions`.
//!
//! Cache hits are read from usage when present. Prices come from the loaded
//! model settings, not from this file.
//! HTTP goes through [`crate::vendor_request::VendorRequest`].

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::ToolRegistry;

use super::openai_compat::OpenAiCompatAgent;
use super::Agent;
use conf::{AgentKind, AgentSettings};

const ALIBABA_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions";

/// AliBaba Qwen agent (vendor `AliBaba`).
pub struct AlibabaAgent {
    inner: OpenAiCompatAgent,
}

impl AlibabaAgent {
    /// Build an agent with a resolved API key (not a path, not a filename).
    pub fn new(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::Alibaba { api_model } => api_model.clone(),
            _ => return Err("expected AliBaba agent settings".into()),
        };
        Ok(Self {
            inner: OpenAiCompatAgent::create(
                tools,
                logger,
                agent,
                api_model,
                ALIBABA_URL,
                "AliBaba",
            ),
        })
    }
}

impl Agent for AlibabaAgent {
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
    fn iteration(&mut self, prompt: Option<&str>) -> Result<bool, String> {
        self.inner.iteration(prompt)
    }
    fn finish(&self) {
        self.inner.finish()
    }
    fn dump_session(&self, path: &Path) -> Result<(), String> {
        self.inner.dump_session(path)
    }
    fn reload_session(&mut self, path: &Path) -> Result<(), String> {
        self.inner.reload_session(path)
    }
}
