//! Shared blocking HTTP pipeline for vendor agents.
//!
//! Every agent sends JSON POSTs through this module instead of implementing
//! its own sleep/retry loop. Before each attempt the pipeline waits until the
//! model `rpm` / `tpm` / `rpd` windows allow another call (`-1` skips that
//! check). Failed transport, HTTP 429 and HTTP 503 are retried up to 24 times;
//! other HTTP errors are returned immediately. The crate stays on blocking
//! `reqwest` — no async runtime and no extra retry middleware.

use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use logger::Logger;
use rand::Rng;
use reqwest::blocking::Client;
use serde_json::Value;

/// Maximum send attempts for one logical model call (matches the old Google loop).
const MAX_ATTEMPTS: u32 = 24;
/// Sliding window for tokens-per-minute.
const TPM_WINDOW: Duration = Duration::from_secs(60);
/// Sliding window for requests-per-day (24 hours, no vendor timezone).
const RPD_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// HTTP client timeout used by every vendor.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(300);

/// Monotonic clock plus sleep, swapped for a fake in unit tests.
trait Clock: Send + Sync {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

/// Production clock: `Instant` elapsed since construction, real `thread::sleep`.
struct SystemClock {
    origin: Instant,
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        if !duration.is_zero() {
            thread::sleep(duration);
        }
    }
}

/// RPM / TPM / RPD windows for one agent instance (not serialized to dump).
struct RateLimit {
    rpm: i64,
    tpm: i64,
    rpd: i64,
    last_start: Option<Duration>,
    token_window: VecDeque<(Duration, i64)>,
    http_window: VecDeque<Duration>,
}

impl RateLimit {
    fn new(rpm: i64, tpm: i64, rpd: i64) -> Self {
        Self {
            rpm,
            tpm,
            rpd,
            last_start: None,
            token_window: VecDeque::new(),
            http_window: VecDeque::new(),
        }
    }

    /// Sleep until every enabled limit allows a new request, then mark RPM start.
    fn wait(&mut self, clock: &dyn Clock) {
        loop {
            let now = clock.now();
            self.evict(now);
            let delay = self.required_delay(now);
            if delay.is_zero() {
                break;
            }
            clock.sleep(delay);
        }
        self.last_start = Some(clock.now());
    }

    fn record_http(&mut self, clock: &dyn Clock) {
        if self.rpd < 1 {
            return;
        }
        self.http_window.push_back(clock.now());
    }

    fn record_tokens(&mut self, clock: &dyn Clock, tokens: i64) {
        if self.tpm < 1 {
            return;
        }
        self.token_window.push_back((clock.now(), tokens.max(0)));
    }

    fn evict(&mut self, now: Duration) {
        while self
            .token_window
            .front()
            .is_some_and(|(ts, _)| now.saturating_sub(*ts) >= TPM_WINDOW)
        {
            self.token_window.pop_front();
        }
        while self
            .http_window
            .front()
            .is_some_and(|ts| now.saturating_sub(*ts) >= RPD_WINDOW)
        {
            self.http_window.pop_front();
        }
    }

    fn required_delay(&self, now: Duration) -> Duration {
        let mut delay = Duration::ZERO;
        if let Some(d) = self.rpm_delay(now) {
            delay = delay.max(d);
        }
        if let Some(d) = self.tpm_delay(now) {
            delay = delay.max(d);
        }
        if let Some(d) = self.rpd_delay(now) {
            delay = delay.max(d);
        }
        delay
    }

    fn rpm_delay(&self, now: Duration) -> Option<Duration> {
        if self.rpm < 1 {
            return None;
        }
        let last = self.last_start?;
        let min_interval = 60.0 / self.rpm as f64;
        let elapsed = now.saturating_sub(last).as_secs_f64();
        if elapsed < min_interval {
            Some(Duration::from_secs_f64(
                (min_interval - elapsed + 1.0).max(0.0),
            ))
        } else {
            None
        }
    }

    fn tpm_delay(&self, now: Duration) -> Option<Duration> {
        if self.tpm < 1 {
            return None;
        }
        let sum: i64 = self.token_window.iter().map(|(_, n)| *n).sum();
        if sum < self.tpm {
            return None;
        }
        let (oldest, _) = self.token_window.front()?;
        Some(TPM_WINDOW.saturating_sub(now.saturating_sub(*oldest)))
    }

    fn rpd_delay(&self, now: Duration) -> Option<Duration> {
        if self.rpd < 1 {
            return None;
        }
        if (self.http_window.len() as i64) < self.rpd {
            return None;
        }
        let oldest = self.http_window.front()?;
        Some(RPD_WINDOW.saturating_sub(now.saturating_sub(*oldest)))
    }
}

/// Retry classification separate from the rate-limit windows.
enum RetryAction {
    Success,
    WaitAndRetry,
    SleepRandomAndRetry,
    Fail,
}

impl RetryAction {
    fn from_status(status: u16) -> Self {
        match status {
            429 => RetryAction::WaitAndRetry,
            503 => RetryAction::SleepRandomAndRetry,
            s if (200..300).contains(&s) => RetryAction::Success,
            _ => RetryAction::Fail,
        }
    }
}

/// Blocking request layer: rate limits + retries for one agent instance.
pub struct VendorRequest {
    client: Client,
    limiter: RateLimit,
    logger: Arc<dyn Logger>,
    clock: Arc<dyn Clock>,
}

impl VendorRequest {
    /// Build a layer from model `rpm` / `tpm` / `rpd` (`-1` disables a limit).
    pub fn new(rpm: i64, tpm: i64, rpd: i64, logger: Arc<dyn Logger>) -> Self {
        Self::new_with_clock(
            rpm,
            tpm,
            rpd,
            logger,
            Arc::new(SystemClock {
                origin: Instant::now(),
            }),
        )
    }

    fn new_with_clock(
        rpm: i64,
        tpm: i64,
        rpd: i64,
        logger: Arc<dyn Logger>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(CLIENT_TIMEOUT)
                .build()
                .expect("reqwest client"),
            limiter: RateLimit::new(rpm, tpm, rpd),
            logger,
            clock,
        }
    }

    /// Remember `input + output` tokens of a successful response for TPM.
    pub fn record_tokens(&mut self, tokens: i64) {
        let clock = self.clock.clone();
        self.limiter.record_tokens(&*clock, tokens);
    }

    /// POST JSON, retry on 429 / 503 / transport errors, return a 2xx body.
    pub fn send_json(
        &mut self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<String, String> {
        let client = self.client.clone();
        let url = url.to_string();
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let body = body.clone();
        self.execute(
            move || match json_post(&client, &url, &headers, &body).send() {
                Err(e) => Err(e.to_string()),
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    resp.text()
                        .map(|text| (status, text))
                        .map_err(|e| e.to_string())
                }
            },
        )
    }

    fn execute<F>(&mut self, mut send_once: F) -> Result<String, String>
    where
        F: FnMut() -> Result<(u16, String), String>,
    {
        let clock = self.clock.clone();
        let mut last_err = "Request failed after max retries".to_string();
        for attempt in 1..=MAX_ATTEMPTS {
            self.limiter.wait(&*clock);
            match send_once() {
                Err(e) => {
                    last_err = format!("⚠️ {attempt}-attempt request failed: {e}");
                    self.logger.log_line(&last_err);
                }
                Ok((status, body)) => {
                    self.limiter.record_http(&*clock);
                    match RetryAction::from_status(status) {
                        RetryAction::Success => return Ok(body),
                        RetryAction::WaitAndRetry => {
                            self.logger.log_str(" fail 429: model limit exceeded!");
                            last_err = "fail 429: model limit exceeded!".into();
                        }
                        RetryAction::SleepRandomAndRetry => {
                            let delay_secs = rand::thread_rng().gen_range(5..=20);
                            self.logger
                                .log_str(&format!(" fail 503: waiting {delay_secs}s..."));
                            clock.sleep(Duration::from_secs(delay_secs));
                            last_err = "fail 503".into();
                        }
                        RetryAction::Fail => {
                            return Err(format!("HTTP {status}: {body}"));
                        }
                    }
                }
            }
        }
        Err(last_err)
    }
}

/// Headers first, then `.json()`.
///
/// `RequestBuilder::header` *appends*. Calling `.json()` first sets
/// `Content-Type: application/json`, and a later `Content-Type` header
/// becomes a duplicate (`application/json, application/json`). xAI then
/// answers HTTP 415. `.json()` itself skips `Content-Type` when it is
/// already present, so the original agent order (headers → json) is safe.
fn json_post(
    client: &Client,
    url: &str,
    headers: &[(String, String)],
    body: &Value,
) -> reqwest::blocking::RequestBuilder {
    let mut builder = client.post(url);
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    builder.json(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn content_types(headers: &[(&str, &str)]) -> Vec<String> {
        let headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let req = json_post(
            &Client::new(),
            "https://api.x.ai/v1/chat/completions",
            &headers,
            &json!({"model": "grok-4.6"}),
        )
        .build()
        .expect("build json post");
        req.headers()
            .get_all(reqwest::header::CONTENT_TYPE)
            .iter()
            .map(|v| v.to_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn json_post_sets_single_content_type_when_header_missing() {
        let types = content_types(&[("Authorization", "Bearer test")]);
        assert_eq!(types, ["application/json"]);
    }

    #[test]
    fn json_post_does_not_duplicate_content_type_from_headers() {
        let types = content_types(&[
            ("Authorization", "Bearer test"),
            ("x-grok-conv-id", "conv"),
            ("Content-Type", "application/json"),
        ]);
        assert_eq!(types, ["application/json"]);
    }

    fn prompt_xai_api_key() -> String {
        eprint!("Enter xAI API key: ");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let mut key = String::new();
        std::io::stdin()
            .read_line(&mut key)
            .expect("read xAI API key from stdin");
        key.trim().to_string()
    }

    fn grok_headers<'a>(auth: &'a str, conv_id: &'a str) -> [(&'a str, &'a str); 3] {
        [
            ("Authorization", auth),
            ("x-grok-conv-id", conv_id),
            ("Content-Type", "application/json"),
        ]
    }

    /// Live Grok 4.6 calls. Run:
    /// `cargo test -p harness grok_46_json_content_type_live -- --ignored --nocapture`
    #[test]
    #[ignore = "live xAI network; paste the API key when prompted"]
    fn grok_46_json_content_type_live() {
        let api_key = prompt_xai_api_key();
        assert!(!api_key.is_empty(), "xAI API key must not be empty");

        let logger: Arc<dyn Logger> = Arc::new(logger::NullLogger);
        let mut http = VendorRequest::new(-1, -1, -1, logger);
        let auth = format!("Bearer {api_key}");
        let conv_id = uuid::Uuid::new_v4().to_string();
        let headers = grok_headers(&auth, &conv_id);

        let ok = http
            .send_json(
                "https://api.x.ai/v1/chat/completions",
                &headers,
                &json!({
                    "model": "grok-4.6",
                    "messages": [{
                        "role": "user",
                        "content": "Reply with the single word pong."
                    }],
                }),
            )
            .expect("grok-4.6 JSON POST should succeed");
        let parsed: Value = serde_json::from_str(&ok).expect("grok JSON body");
        assert!(
            parsed.get("choices").and_then(|c| c.as_array()).is_some(),
            "expected choices in grok-4.6 response: {ok}"
        );

        let err = http
            .send_json(
                "https://api.x.ai/v1/chat/completions",
                &headers,
                &json!({
                    "model": "this-model-does-not-exist",
                    "messages": [{"role": "user", "content": "ping"}],
                }),
            )
            .expect_err("unknown model must fail");
        assert!(
            !err.contains("HTTP 415"),
            "unknown model failed with 415 (Content-Type not accepted): {err}"
        );
        assert!(
            err.starts_with("HTTP "),
            "expected an HTTP error, got: {err}"
        );
    }
}
