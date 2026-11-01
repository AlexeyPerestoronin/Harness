//! Shared asynchronous HTTP pipeline for vendor agents.
//!
//! Every agent sends JSON POSTs through this module instead of implementing
//! its own sleep/retry loop. Before each attempt the pipeline waits until the
//! model `rpm` / `tpm` / `rpd` windows allow another call (`-1` skips that
//! check). Failed transport, HTTP 429 and HTTP 503 are retried up to 24 times;
//! other HTTP errors are returned immediately.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use interactions::{Interaction, OutputType};
use rand::Rng;
use reqwest::{Client, RequestBuilder, Response};
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

/// Maximum send attempts for one logical model call (matches the old Google loop).
const MAX_ATTEMPTS: usize = 24;
/// Sliding window for tokens-per-minute.
const TPM_WINDOW: Duration = Duration::from_secs(60);
/// Sliding window for requests-per-day (24 hours, no vendor timezone).
const RPD_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);

/// Context for one [`RequestPipeline::execute`] attempt.
#[derive(Debug, Clone)]
pub struct PipelineContext {
    /// 1-based attempt number.
    pub attempt: usize,
    /// Request URL.
    pub url: String,
    /// Monotonic start time of the attempt.
    pub started_at: std::time::Instant,
    /// Interval at which [`DurAction`]s run while the request is in flight.
    pub tick: Duration,
    /// Cancellation token shared with the agent loop.
    pub cancel: CancellationToken,
}

/// Failure of a pipeline attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    /// The caller cancelled the request.
    Cancelled,
    /// The HTTP transport failed.
    Transport(String),
    /// A pre/dur/post action failed.
    Hook(String),
}

/// Action executed before the request is sent.
#[async_trait::async_trait]
pub trait PreAction: Send + Sync {
    async fn execute(&self, ctx: &PipelineContext) -> Result<(), PipelineError>;
}

/// Action executed once per second while the request is in flight.
#[async_trait::async_trait]
pub trait DurAction: Send + Sync {
    async fn execute(&self, ctx: &PipelineContext) -> Result<(), PipelineError>;
}

/// Action executed after the response headers/status arrive.
#[async_trait::async_trait]
pub trait PostAction: Send + Sync {
    async fn execute(
        &self,
        ctx: &PipelineContext,
        response: &Response,
    ) -> Result<(), PipelineError>;
}

/// One cancellable request attempt.
#[async_trait::async_trait]
pub trait RequestPipeline<T>: Send + Sync
where
    T: Serialize + Send + Sync,
{
    fn build(&self, client: &Client, payload: &T) -> RequestBuilder;

    fn pre_actions(&self) -> &[Arc<dyn PreAction>];
    fn dur_actions(&self) -> &[Arc<dyn DurAction>];
    fn post_actions(&self) -> &[Arc<dyn PostAction>];

    fn add_pre(&mut self, action: Arc<dyn PreAction>);
    fn add_dur(&mut self, action: Arc<dyn DurAction>);
    fn add_post(&mut self, action: Arc<dyn PostAction>);

    async fn execute(
        &self,
        client: &Client,
        payload: &T,
        ctx: PipelineContext,
    ) -> Result<Response, PipelineError> {
        for action in self.pre_actions() {
            action.execute(&ctx).await?;
        }

        let request = self.build(client, payload).send();
        tokio::pin!(request);

        let mut ticker = tokio::time::interval(ctx.tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let response = loop {
            tokio::select! {
                _ = ctx.cancel.cancelled() => {
                    return Err(PipelineError::Cancelled);
                }
                _ = ticker.tick() => {
                    for action in self.dur_actions() {
                        action.execute(&ctx).await?;
                    }
                }
                result = &mut request => {
                    break result.map_err(|e| PipelineError::Transport(e.to_string()))?;
                }
            }
        };

        for action in self.post_actions() {
            action.execute(&ctx, &response).await?;
        }

        Ok(response)
    }
}

/// JSON POST implementation of [`RequestPipeline`].
pub struct JsonPipeline<T>
where
    T: Serialize + Send + Sync,
{
    url: String,
    extra_headers: Vec<(String, String)>,
    pre: Vec<Arc<dyn PreAction>>,
    dur: Vec<Arc<dyn DurAction>>,
    post: Vec<Arc<dyn PostAction>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T> JsonPipeline<T>
where
    T: Serialize + Send + Sync,
{
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            extra_headers: Vec::new(),
            pre: Vec::new(),
            dur: Vec::new(),
            post: Vec::new(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.extra_headers.push((key.into(), value.into()));
        self
    }
}

#[async_trait::async_trait]
impl<T> RequestPipeline<T> for JsonPipeline<T>
where
    T: Serialize + Send + Sync,
{
    fn build(&self, client: &Client, payload: &T) -> RequestBuilder {
        let mut req = client.post(&self.url).json(payload);
        for (key, value) in &self.extra_headers {
            req = req.header(key, value);
        }
        req
    }

    fn pre_actions(&self) -> &[Arc<dyn PreAction>] {
        &self.pre
    }

    fn dur_actions(&self) -> &[Arc<dyn DurAction>] {
        &self.dur
    }

    fn post_actions(&self) -> &[Arc<dyn PostAction>] {
        &self.post
    }

    fn add_pre(&mut self, action: Arc<dyn PreAction>) {
        self.pre.push(action);
    }

    fn add_dur(&mut self, action: Arc<dyn DurAction>) {
        self.dur.push(action);
    }

    fn add_post(&mut self, action: Arc<dyn PostAction>) {
        self.post.push(action);
    }
}

/// RPM / TPM / RPD windows for one agent instance (not serialized to dump).
struct RateLimit {
    rpm: i64,
    tpm: i64,
    rpd: i64,
    last_start: Option<tokio::time::Instant>,
    token_window: VecDeque<(tokio::time::Instant, i64)>,
    http_window: VecDeque<tokio::time::Instant>,
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
    async fn wait(&mut self, cancel: &CancellationToken) -> Result<(), String> {
        loop {
            if cancel.is_cancelled() {
                return Err("request cancelled".into());
            }
            let now = tokio::time::Instant::now();
            self.evict(now);
            let delay = self.required_delay(now);
            if delay.is_zero() {
                break;
            }
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err("request cancelled".into());
                }
                _ = tokio::time::sleep(delay) => {}
            }
        }
        self.last_start = Some(tokio::time::Instant::now());
        Ok(())
    }

    fn record_http(&mut self) {
        if self.rpd < 1 {
            return;
        }
        self.http_window.push_back(tokio::time::Instant::now());
    }

    fn record_tokens(&mut self, tokens: i64) {
        if self.tpm < 1 {
            return;
        }
        self.token_window
            .push_back((tokio::time::Instant::now(), tokens.max(0)));
    }

    fn evict(&mut self, now: tokio::time::Instant) {
        while self
            .token_window
            .front()
            .is_some_and(|(ts, _)| now.saturating_duration_since(*ts) >= TPM_WINDOW)
        {
            self.token_window.pop_front();
        }
        while self
            .http_window
            .front()
            .is_some_and(|ts| now.saturating_duration_since(*ts) >= RPD_WINDOW)
        {
            self.http_window.pop_front();
        }
    }

    fn required_delay(&self, now: tokio::time::Instant) -> Duration {
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

    fn rpm_delay(&self, now: tokio::time::Instant) -> Option<Duration> {
        if self.rpm < 1 {
            return None;
        }
        let last = self.last_start?;
        let min_interval = 60.0 / self.rpm as f64;
        let elapsed = now.saturating_duration_since(last).as_secs_f64();
        if elapsed < min_interval {
            Some(Duration::from_secs_f64(
                (min_interval - elapsed + 1.0).max(0.0),
            ))
        } else {
            None
        }
    }

    fn tpm_delay(&self, now: tokio::time::Instant) -> Option<Duration> {
        if self.tpm < 1 {
            return None;
        }
        let sum: i64 = self.token_window.iter().map(|(_, n)| *n).sum();
        if sum < self.tpm {
            return None;
        }
        let (oldest, _) = self.token_window.front()?;
        Some(
            TPM_WINDOW.saturating_sub(now.saturating_duration_since(*oldest)),
        )
    }

    fn rpd_delay(&self, now: tokio::time::Instant) -> Option<Duration> {
        if self.rpd < 1 {
            return None;
        }
        if (self.http_window.len() as i64) < self.rpd {
            return None;
        }
        let oldest = self.http_window.front()?;
        Some(
            RPD_WINDOW.saturating_sub(now.saturating_duration_since(*oldest)),
        )
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

/// Console progress action executed by [`RequestPipeline`] once per second.
struct WaitingAction {
    interaction: Arc<dyn Interaction>,
}

impl WaitingAction {
    fn new(interaction: Arc<dyn Interaction>) -> Self {
        Self { interaction }
    }
}

#[async_trait::async_trait]
impl DurAction for WaitingAction {
    async fn execute(&self, ctx: &PipelineContext) -> Result<(), PipelineError> {
        let text = format!(
            "response waiting time: {}{}",
            format_waiting_time(ctx.started_at.elapsed().as_secs()),
            "          " // 10 spaces for override tail
        );
        self.interaction
            .output(&text, false, OutputType::RewriteLine)
            .await
            .map_err(|e| PipelineError::Hook(e.to_string()))
    }
}

/// Asynchronous request layer: rate limits + retries for one agent instance.
pub struct VendorRequest {
    client: Client,
    limiter: RateLimit,
    interaction: Arc<dyn Interaction>,
}

impl VendorRequest {
    /// Build a layer from model `rpm` / `tpm` / `rpd` (`-1` disables a limit).
    pub fn new(rpm: i64, tpm: i64, rpd: i64, interaction: Arc<dyn Interaction>) -> Self {
        Self {
            client: Client::builder()
                .build()
                .expect("failed to build reqwest client"),
            limiter: RateLimit::new(rpm, tpm, rpd),
            interaction,
        }
    }

    /// Remember `input + output` tokens of a successful response for TPM.
    pub fn record_tokens(&mut self, tokens: i64) {
        self.limiter.record_tokens(tokens);
    }

    /// POST JSON, retry on 429 / 503 / transport errors, return a 2xx body.
    pub async fn send_json(
        &mut self,
        url: &str,
        headers: &[(&str, &str)],
        body: &Value,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let mut pipeline = JsonPipeline::<Value>::new(url);
        for (key, value) in headers {
            pipeline = pipeline.header(key, value);
        }
        pipeline.add_dur(Arc::new(WaitingAction::new(self.interaction.clone())));

        let body = body.clone();
        let client = self.client.clone();
        let mut last_err = "Request failed after max retries".to_string();

        for attempt in 1..=MAX_ATTEMPTS {
            if cancel.is_cancelled() {
                return Err("request cancelled".into());
            }
            self.limiter.wait(cancel).await?;

            let ctx = PipelineContext {
                attempt,
                url: url.to_string(),
                started_at: std::time::Instant::now(),
                tick: Duration::from_secs(1),
                cancel: cancel.clone(),
            };

            let response = match pipeline.execute(&client, &body, ctx.clone()).await {
                Ok(response) => response,
                Err(PipelineError::Cancelled) => return Err("request cancelled".into()),
                Err(PipelineError::Transport(e)) => {
                    last_err = format!("⚠️ {attempt}-attempt request failed: {e}");
                    self.log_line(&last_err).await;
                    continue;
                }
                Err(PipelineError::Hook(e)) => {
                    last_err = format!("⚠️ {attempt}-attempt request hook failed: {e}");
                    self.log_line(&last_err).await;
                    continue;
                }
            };

            let status = response.status().as_u16();
            let body = match self.read_response_body(response, &ctx).await {
                Ok(body) => body,
                Err(PipelineError::Cancelled) => return Err("request cancelled".into()),
                Err(PipelineError::Transport(e)) => {
                    last_err = format!("⚠️ {attempt}-attempt request failed: {e}");
                    self.log_line(&last_err).await;
                    continue;
                }
                Err(PipelineError::Hook(e)) => {
                    last_err = format!("⚠️ {attempt}-attempt request hook failed: {e}");
                    self.log_line(&last_err).await;
                    continue;
                }
            };
            self.limiter.record_http();

            match RetryAction::from_status(status) {
                RetryAction::Success => return Ok(body),
                RetryAction::WaitAndRetry => {
                    self.log_cont(" fail 429: model limit exceeded!").await;
                    last_err = "fail 429: model limit exceeded!".into();
                }
                RetryAction::SleepRandomAndRetry => {
                    let delay_secs = rand::thread_rng().gen_range(5..=20);
                    self.log_cont(&format!(" fail 503: waiting {delay_secs}s..."))
                        .await;
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return Err("request cancelled".into());
                        }
                        _ = tokio::time::sleep(Duration::from_secs(delay_secs)) => {}
                    }
                    last_err = "fail 503".into();
                }
                RetryAction::Fail => {
                    return Err(format!("HTTP {status}: {body}"));
                }
            }
        }

        Err(last_err)
    }

    /// Read the response body while keeping the console waiting ticker alive.
    ///
    /// [`RequestPipeline::execute`] stops running [`DurAction`]s as soon as
    /// the response headers arrive. Non-streaming vendors spend most of the
    /// wall-clock time in [`Response::text`], so the waiting indicator is
    /// continued here with the same tick and cancellation semantics.
    async fn read_response_body(
        &self,
        response: Response,
        ctx: &PipelineContext,
    ) -> Result<String, PipelineError> {
        let read_body = async { response.text().await.map_err(|e| transport_error(&e)) };
        tokio::pin!(read_body);

        let mut ticker = tokio::time::interval(ctx.tick);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let waiting = WaitingAction::new(self.interaction.clone());

        loop {
            tokio::select! {
                _ = ctx.cancel.cancelled() => {
                    return Err(PipelineError::Cancelled);
                }
                _ = ticker.tick() => {
                    waiting.execute(ctx).await?;
                }
                result = &mut read_body => {
                    return result.map_err(|e| PipelineError::Transport(e));
                }
            }
        }
    }

    async fn log_line(&self, message: &str) {
        let _ = self
            .interaction
            .output(message, true, OutputType::NewLine)
            .await;
    }

    async fn log_cont(&self, message: &str) {
        let _ = self
            .interaction
            .output(message, true, OutputType::ContinueLine)
            .await;
    }
}

/// Top-level reqwest text plus the source chain.
///
/// `Error`'s `Display` is only `error decoding response body` / `error sending
/// request`. The useful cause (`operation timed out`, `connection refused`)
/// sits in `source()`.
fn transport_error(err: &reqwest::Error) -> String {
    let mut message = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        let text = inner.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = inner.source();
    }
    message
}

/// Format a duration in whole seconds as `X min Y sec`.
fn format_waiting_time(secs: u64) -> String {
    format!("{} min {} sec", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct RecordingInteraction {
        lines: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl Interaction for RecordingInteraction {
        async fn output(
            &self,
            message: &str,
            _write_log: bool,
            _new_line: OutputType,
        ) -> Result<(), interactions::InteractionError> {
            self.lines.lock().unwrap().push(message.to_string());
            Ok(())
        }

        async fn input(
            &self,
            _request: &interactions::InteractionRequest,
            _write_log: bool,
        ) -> Result<String, interactions::InteractionError> {
            unreachable!("input is not used by request waiting tests")
        }
    }

    struct SlowServer {
        url: String,
        stop: Arc<AtomicBool>,
    }

    impl SlowServer {
        fn spawn(body: &'static str, body_delay: Duration) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stop_thread = Arc::clone(&stop);

            std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buf = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => request.extend_from_slice(&buf[..n]),
                        Err(_) => return,
                    }
                }

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if stream.write_all(response.as_bytes()).is_err() {
                    return;
                }
                if stream.flush().is_err() {
                    return;
                }

                let deadline = std::time::Instant::now() + body_delay;
                while std::time::Instant::now() < deadline {
                    if stop_thread.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                if !stop_thread.load(Ordering::SeqCst) {
                    let _ = stream.write_all(body.as_bytes());
                }
            });

            Self {
                url: format!("http://{addr}"),
                stop,
            }
        }
    }

    #[tokio::test]
    async fn rate_limit_wait_respects_cancellation() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut limiter = RateLimit::new(60, -1, -1);
        assert!(limiter.wait(&cancel).await.is_err());
    }

    #[test]
    fn retry_classification_is_preserved() {
        assert!(matches!(RetryAction::from_status(200), RetryAction::Success));
        assert!(matches!(RetryAction::from_status(299), RetryAction::Success));
        assert!(matches!(RetryAction::from_status(429), RetryAction::WaitAndRetry));
        assert!(matches!(
            RetryAction::from_status(503),
            RetryAction::SleepRandomAndRetry
        ));
        assert!(matches!(RetryAction::from_status(400), RetryAction::Fail));
        assert!(matches!(RetryAction::from_status(500), RetryAction::Fail));
    }

    #[tokio::test]
    async fn pipeline_execute_cancels_when_token_is_cancelled() {
        let pipeline = JsonPipeline::<Value>::new("http://127.0.0.1:1/v1/test")
            .header("Content-Type", "application/json");
        let client = Client::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = pipeline
            .execute(
                &client,
                &serde_json::json!({ "prompt": "x" }),
                PipelineContext {
                    attempt: 1,
                    url: "http://127.0.0.1:1/v1/test".into(),
                    started_at: std::time::Instant::now(),
                    tick: Duration::from_secs(1),
                    cancel,
                },
            )
            .await;
        assert_eq!(result.unwrap_err(), PipelineError::Cancelled);
    }

    #[tokio::test]
    async fn read_response_body_keeps_ticking_while_body_is_slow() {
        let server = SlowServer::spawn("{}", Duration::from_millis(2600));
        let interaction = Arc::new(RecordingInteraction::default());
        let vendor = VendorRequest::new(-1, -1, -1, interaction.clone());
        let client = Client::builder().no_proxy().build().unwrap();
        let response = client.get(&server.url).send().await.unwrap();

        let ctx = PipelineContext {
            attempt: 1,
            url: server.url.clone(),
            started_at: std::time::Instant::now(),
            tick: Duration::from_secs(1),
            cancel: CancellationToken::new(),
        };

        let body = vendor.read_response_body(response, &ctx).await.unwrap();
        assert_eq!(body, "{}");

        let lines = interaction.lines.lock().unwrap();
        let total_seconds: Vec<u64> = lines
            .iter()
            .map(|line| {
                let rest = line.strip_prefix("response waiting time: ").unwrap();
                let parts: Vec<&str> = rest.split_whitespace().collect();
                let minutes: u64 = parts[0].parse().unwrap();
                let seconds: u64 = parts[2].parse().unwrap();
                minutes * 60 + seconds
            })
            .collect();

        assert!(
            total_seconds.len() >= 2,
            "expected at least two ticks, got {lines:?}"
        );
        assert!(
            total_seconds.windows(2).any(|pair| pair[0] != pair[1]),
            "expected the counter to advance, got {total_seconds:?}"
        );
    }

    #[tokio::test]
    async fn read_response_body_cancels_when_token_is_cancelled() {
        let server = SlowServer::spawn("{}", Duration::from_secs(30));
        let interaction = Arc::new(RecordingInteraction::default());
        let interaction_for_task = interaction.clone();
        let url = server.url.clone();
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let task = tokio::spawn(async move {
            let vendor = VendorRequest::new(-1, -1, -1, interaction_for_task);
            let client = Client::builder().no_proxy().build().unwrap();
            let response = client.get(&url).send().await.unwrap();
            let ctx = PipelineContext {
                attempt: 1,
                url,
                started_at: std::time::Instant::now(),
                tick: Duration::from_secs(1),
                cancel: cancel_for_task,
            };
            vendor.read_response_body(response, &ctx).await
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while interaction.lines.lock().unwrap().is_empty() {
            if std::time::Instant::now() > deadline {
                panic!("waiting indicator was not printed before cancellation");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        cancel.cancel();
        let result = task.await.unwrap();
        server.stop.store(true, Ordering::SeqCst);

        assert_eq!(result.unwrap_err(), PipelineError::Cancelled);
    }
}
