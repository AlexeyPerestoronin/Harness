//! DeepSeek agent over OpenAI-compatible `chat/completions`.
//!
//! Models: `deepseek-chat` (V3), `deepseek-reasoner` (R1), `deepseek-flash`
//! and `deepseek-v4-pro`. Prefix cache is automatic;
//! `prompt_cache_hit_tokens` maps to `UsageStats.cached_tokens`.
//! HTTP goes through [`crate::vendor_request::VendorRequest`].

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::ToolRegistry;

use super::openai_compat::{OpenAiCompatAgent, TokenPrices};
use super::Agent;
use conf::{AgentKind, AgentSettings};

const DEEPSEEK_URL: &str = "https://api.deepseek.com/chat/completions";

/// DeepSeek agent (vendor `DeepSeek`).
pub struct DeepSeekAgent {
    inner: OpenAiCompatAgent,
}

impl DeepSeekAgent {
    /// Build an agent with a resolved API key (not a path, not a filename).
    pub fn new(
        tools: Arc<ToolRegistry>,
        logger: Arc<dyn Logger>,
        agent: &AgentSettings,
    ) -> Result<Self, String> {
        let api_model = match &agent.kind {
            AgentKind::DeepSeek { api_model } => api_model.clone(),
            _ => return Err("expected DeepSeek agent settings".into()),
        };
        let prices = prices_for(&api_model);
        Ok(Self {
            inner: OpenAiCompatAgent::create(
                tools,
                logger,
                agent,
                api_model,
                DEEPSEEK_URL,
                "DeepSeek",
                prices,
            ),
        })
    }
}

impl Agent for DeepSeekAgent {
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

/// Official DeepSeek USD / 1M that matches the task input/output table:
/// cache hit is the published price (`$0.014` / `$0.14`), not a guessed 90% off.
/// `deepseek-flash` / `deepseek-v4-pro` have no separate table — V3 rates.
fn prices_for(api_model: &str) -> TokenPrices {
    if api_model == "deepseek-reasoner" {
        TokenPrices {
            input: 0.55,
            cache_hit: 0.14,
            output: 2.19,
        }
    } else {
        TokenPrices {
            input: 0.14,
            cache_hit: 0.014,
            output: 0.28,
        }
    }
}
