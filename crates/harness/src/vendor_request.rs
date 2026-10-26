//! Shared blocking HTTP pipeline for vendor agents.
//!
//! Every agent sends JSON POSTs through this module instead of implementing
//! its own sleep/retry loop. Before each attempt the pipeline waits until the
//! model `rpm` / `tpm` / `rpd` windows allow another call (`-1` skips that
//! check). Failed transport, HTTP 429 and HTTP 503 are retried up to 24 times;
//! other HTTP errors are returned immediately. The crate stays on blocking
//! `reqwest` — no async runtime and no extra retry middleware.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
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
            // No response timeout: the model may keep thinking indefinitely.
            client: Client::builder().build().expect("reqwest client"),
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

    fn execute<F>(&mut self, send_once: F) -> Result<String, String>
    where
        F: FnMut() -> Result<(u16, String), String> + Send,
    {
        let mut stdout = io::stdout();
        self.execute_with_writer(send_once, &mut stdout)
    }

    fn execute_with_writer<F, W>(&mut self, mut send_once: F, out: &mut W) -> Result<String, String>
    where
        F: FnMut() -> Result<(u16, String), String> + Send,
        W: Write,
    {
        let clock = self.clock.clone();
        let mut last_err = "Request failed after max retries".to_string();
        for attempt in 1..=MAX_ATTEMPTS {
            self.limiter.wait(&*clock);
            match run_attempt(&mut send_once, out) {
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

/// Format a duration in whole seconds as `X min Y sec`.
fn format_waiting_time(secs: u64) -> String {
    format!("{} min {} sec", secs / 60, secs % 60)
}

/// Build one stopwatch line. `first` controls the leading `\n` / `\r`, and
/// shorter lines are padded with spaces up to the previous visible length.
fn waiting_update(secs: u64, first: bool, previous_len: usize) -> (String, usize) {
    let text = format!("response waiting time: {}", format_waiting_time(secs));
    let prefix = if first { "\n\n" } else { "\r" };
    let padding = " ".repeat(previous_len.saturating_sub(text.len()));
    let line = format!("{prefix}{text}{padding}");
    (line, previous_len.max(text.len()))
}

/// Run one `send_once` in a scoped thread and wait for its result, printing
/// the console stopwatch every second while the request is in flight.
fn run_attempt<F, W>(send_once: &mut F, out: &mut W) -> Result<(u16, String), String>
where
    F: FnMut() -> Result<(u16, String), String> + Send,
    W: Write,
{
    let (tx, rx) = mpsc::channel::<Result<(u16, String), String>>();
    let started = Instant::now();
    let mut first = true;
    let mut previous_len = 0;

    thread::scope(|scope| {
        scope.spawn(move || {
            let result = send_once();
            let _ = tx.send(result);
        });

        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => {
                    let (line, new_len) =
                        waiting_update(started.elapsed().as_secs(), first, previous_len);
                    let _ = out.write_all(line.as_bytes());
                    let _ = out.flush();
                    first = false;
                    previous_len = new_len;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("response waiting thread failed".to_string());
                }
            }
        }
    })
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
