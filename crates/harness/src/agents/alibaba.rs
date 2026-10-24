//! AliBaba (Qwen) agent over DashScope OpenAI-compatible `chat/completions`.
//!
//! Cache hits are read from usage when present. Cache-hit USD is `0` because
//! an official per-SKU cache price was not taken from a fetched price list.
//! HTTP goes through [`crate::vendor_request::VendorRequest`].

use std::path::Path;
use std::sync::Arc;

use logger::Logger;
use tools::ToolRegistry;

use super::openai_compat::{OpenAiCompatAgent, TokenPrices};
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
        let prices = prices_for(&api_model);
        Ok(Self {
            inner: OpenAiCompatAgent::create(
                tools,
                logger,
                agent,
                api_model,
                ALIBABA_URL,
                "AliBaba",
                prices,
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

/// Input/output USD per 1M from the task table. Cache hit is 0: no fetched
/// DashScope cache-hit price for these SKUs (do not invent a coefficient).
fn prices_for(api_model: &str) -> TokenPrices {
    let (input, output) = match api_model {
        "qwen-plus" => (0.11, 0.28),
        "qwen-max" => (1.65, 4.40),
        _ => (0.04, 0.08),
    };
    TokenPrices {
        input,
        cache_hit: 0.0,
        output,
    }
}

#[cfg(test)]
mod tests {
    use super::super::openai_compat::{parse_chat_completion, usd_cost};
    use super::*;

    #[test]
    fn qwen_max_prices_and_zero_cache_hit() {
        let p = prices_for("qwen-max");
        assert_eq!(p.input, 1.65);
        assert_eq!(p.output, 4.40);
        assert_eq!(p.cache_hit, 0.0);
        let usd = usd_cost(100, 40, 20, p);
        let expected = 60.0 / 1e6 * 1.65 + 20.0 / 1e6 * 4.40;
        assert!((usd - expected).abs() < 1e-12);
    }

    #[test]
    fn qwen_max_tool_calls_from_fixture() {
        let text = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_max",
                        "type": "function",
                        "function": {"name": "list_folder", "arguments": "{\"path\":\".\"}"}
                    }]
                }
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3}
        }"#;
        let parsed = parse_chat_completion(text, "AliBaba").unwrap();
        assert_eq!(parsed.tool_calls[0].id, "call_max");
        assert_eq!(parsed.tool_calls[0].name, "list_folder");
        assert_eq!(parsed.tool_calls[0].arguments, r#"{"path":"."}"#);
    }
}
