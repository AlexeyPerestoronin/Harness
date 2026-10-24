//! DeepSeek agent over OpenAI-compatible `chat/completions`.
//!
//! Models: `deepseek-chat` (V3) and `deepseek-reasoner` (R1). Prefix cache is
//! automatic; `prompt_cache_hit_tokens` maps to `UsageStats.cached_tokens`.
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

#[cfg(test)]
mod tests {
    use super::super::openai_compat::{parse_chat_completion, usd_cost};
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn v3_prices_match_table() {
        let p = prices_for("deepseek-chat");
        assert_eq!(p.input, 0.14);
        assert_eq!(p.output, 0.28);
        assert_eq!(p.cache_hit, 0.014);
    }

    #[test]
    fn live_deepseek_chat_and_cache_fields() {
        let key = match read_live_key() {
            Some(k) => k,
            None => {
                eprintln!("skip live DeepSeek test: API_DEEP_SEEK not found");
                return;
            }
        };
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("client");
        let prefix = "DeepSeek cache prefix. ".repeat(80);
        let body = json!({
            "model": "deepseek-chat",
            "messages": [
                {"role": "user", "content": format!("{prefix}Reply with the single word pong.")}
            ],
        });
        let first = post_chat(&client, &key, &body);
        let parsed = parse_chat_completion(&first, "DeepSeek").expect("parse first");
        assert!(
            parsed
                .content
                .as_deref()
                .is_some_and(|c| !c.trim().is_empty()),
            "empty DeepSeek content: {first}"
        );
        let second = post_chat(&client, &key, &body);
        let parsed2 = parse_chat_completion(&second, "DeepSeek").expect("parse second");
        assert!(parsed2.prompt_tokens > 0, "second usage missing: {second}");
        let usd = usd_cost(
            parsed2.prompt_tokens,
            parsed2.cached_tokens,
            parsed2.completion_tokens,
            prices_for("deepseek-chat"),
        );
        assert!(usd >= 0.0);
        eprintln!(
            "live DeepSeek: input={} output={} cached={} usd={}",
            parsed2.prompt_tokens, parsed2.completion_tokens, parsed2.cached_tokens, usd
        );
    }

    fn read_live_key() -> Option<String> {
        for path in ["API_DEEP_SEEK", "../../API_DEEP_SEEK", "../API_DEEP_SEEK"] {
            if let Ok(text) = std::fs::read_to_string(path) {
                let key = text.trim().to_string();
                if !key.is_empty() {
                    return Some(key);
                }
            }
        }
        None
    }

    fn post_chat(client: &reqwest::blocking::Client, key: &str, body: &Value) -> String {
        let resp = client
            .post(DEEPSEEK_URL)
            .header("Authorization", format!("Bearer {key}"))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .expect("deepseek http");
        let status = resp.status();
        let text = resp.text().expect("deepseek body");
        assert!(status.is_success(), "DeepSeek HTTP {status}: {text}");
        text
    }
}
