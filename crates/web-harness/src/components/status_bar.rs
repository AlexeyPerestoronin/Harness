//! Run status bar.
//!
//! Shows the run state, iteration, token/USD counters and time since the last
//! server response. Session logs are intentionally not displayed.

use crate::state::{parse_iso8601_utc, HarnessClient};
use gloo_timers::future::TimeoutFuture;
use harness_api::RunState;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Status bar component.
#[component]
pub fn StatusBar(client: HarnessClient) -> impl IntoView {
    let now = RwSignal::new(js_sys::Date::now());

    spawn_local(async move {
        loop {
            TimeoutFuture::new(1_000).await;
            now.set(js_sys::Date::now());
        }
    });

    let state_label = move || {
        let label = match client.status.get().run_state {
            RunState::Idle => "idle",
            RunState::Running => "running",
            RunState::WaitingInput => "waiting_input",
            RunState::Stopping => "stopping",
            RunState::Finished => "finished",
            RunState::Failed => "failed",
            RunState::Stopped => "stopped",
        };
        label.to_string()
    };

    let counters = move || {
        let status = client.status.get();
        match status.summary {
            Some(summary) => format!(
                "итерация {} · {} токенов · ${:.4} (прибл.) / {} (real)",
                summary.iteration,
                summary.tokens,
                summary.usd_approx,
                summary
                    .usd_real
                    .map(|v| format!("${v:.4}"))
                    .unwrap_or_else(|| "—".into())
            ),
            None => "—".into(),
        }
    };

    let elapsed = move || {
        let updated = client
            .status
            .get()
            .updated_at
            .and_then(|value| parse_iso8601_utc(&value));

        match updated {
            Some(timestamp) => {
                let seconds = ((now.get() - timestamp * 1_000.0) / 1_000.0).max(0.0) as u64;
                format!("{seconds} с назад")
            }
            None => "—".into(),
        }
    };

    view! {
        <div class="hw-status" role="status">
            <span class="hw-state">{state_label}</span>
            <span class="hw-counters">{counters}</span>
            <span class="hw-elapsed">{elapsed}</span>
            <Show when=move || !client.connected.get() fallback=|| view! {}>
                <span class="hw-offline">"нет соединения"</span>
            </Show>
        </div>
    }
}
