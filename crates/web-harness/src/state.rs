//! Reactive client state shared by all UI components.
//!
//! `HarnessClient` is a small signal container and is intentionally free of
//! business logic. Components own behaviour; this module guarantees a single
//! reactive source of truth for the panel.

use crate::gateway::{is_terminal_run_state, HarnessGateway};
use harness_api::{RunState, RunStatusResponse};
use leptos::prelude::*;

/// Returns the idle run status used before the first run.
pub fn idle_status() -> RunStatusResponse {
    RunStatusResponse {
        run_state: RunState::Idle,
        started_at: None,
        updated_at: None,
        summary: None,
        pending_input: None,
        result: None,
    }
}

/// Parses the server's ISO-8601 UTC timestamps into seconds since Unix epoch.
///
/// The server contract uses `YYYY-MM-DDTHH:MM:SSZ`; fractional seconds are not
/// part of the current API examples. Returns `None` for malformed input.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn parse_iso8601_utc(value: &str) -> Option<f64> {
    let bytes = value.as_bytes();
    if bytes.len() != 20 {
        return None;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' || bytes[19] != b'Z' {
        return None;
    }

    let year = value[0..4].parse::<i64>().ok()?;
    let month = value[5..7].parse::<u32>().ok()?;
    let day = value[8..10].parse::<u32>().ok()?;
    let hour = value[11..13].parse::<u32>().ok()?;
    let minute = value[14..16].parse::<u32>().ok()?;
    let second = value[17..19].parse::<u32>().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(days as f64 * 86_400.0 + hour as f64 * 3_600.0 + minute as f64 * 60.0 + second as f64)
}

/// Days since Unix epoch from a civil date.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Reactive state of the web client.
#[derive(Clone, Copy)]
pub struct HarnessClient {
    /// Server base URL.
    pub base_url: RwSignal<String>,
    /// Bearer token, if known.
    pub auth_token: RwSignal<Option<String>>,
    /// Relative URL hook of the current run.
    pub run_url: RwSignal<Option<String>>,
    /// Latest known run status.
    pub status: RwSignal<RunStatusResponse>,
    /// Whether the server was reachable during the last request.
    pub connected: RwSignal<bool>,
    /// Consecutive network failures of polling requests.
    pub consecutive_errors: RwSignal<u32>,
    /// Whether the input modal is open.
    pub modal_open: RwSignal<bool>,
    /// Whether the remote config is valid (used by `fs-panel`).
    pub config_valid: RwSignal<bool>,
}

impl HarnessClient {
    /// Creates a client with the given server address and optional token.
    pub fn new(base_url: impl Into<String>, auth_token: Option<String>) -> Self {
        let base_url = HarnessGateway::normalize_base_url(&base_url.into());
        Self {
            base_url: RwSignal::new(base_url),
            auth_token: RwSignal::new(auth_token),
            run_url: RwSignal::new(None),
            status: RwSignal::new(idle_status()),
            connected: RwSignal::new(false),
            consecutive_errors: RwSignal::new(0),
            modal_open: RwSignal::new(false),
            config_valid: RwSignal::new(true),
        }
    }

    /// Builds a fresh gateway from the current signals.
    pub fn gateway(&self) -> HarnessGateway {
        HarnessGateway::new(self.base_url.get_untracked(), self.auth_token.get_untracked())
    }

    /// Resets run-related state before a new run.
    pub fn reset(&self) {
        self.run_url.set(None);
        self.status.set(idle_status());
        self.modal_open.set(false);
        self.consecutive_errors.set(0);
    }

    /// Whether the current status is terminal.
    pub fn is_terminal(&self) -> bool {
        is_terminal_run_state(self.status.get_untracked().run_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso8601_utc_parses_server_format() {
        let ts = parse_iso8601_utc("2026-01-01T12:00:00Z").unwrap();
        assert!(ts > 0.0);
    }

    #[test]
    fn parse_iso8601_utc_rejects_malformed_input() {
        assert!(parse_iso8601_utc("not-a-date").is_none());
        assert!(parse_iso8601_utc("2026-01-01T12:00:00").is_none());
    }

    #[test]
    fn idle_status_is_idle() {
        assert_eq!(idle_status().run_state, RunState::Idle);
        assert!(idle_status().pending_input.is_none());
        assert!(idle_status().result.is_none());
    }
}
