//! Prompt composer with Start/Stop controls.
//!
//! Start sends `POST /run` and stores the returned `runUrl`; Stop sends
//! `POST /run/stop`. Button availability follows the run-state contract.

use crate::components::connect_dialog::clear_token;
use crate::gateway::GatewayError;
use crate::state::HarnessClient;
use harness_api::RunState;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Prompt composer component.
#[component]
pub fn PromptComposer(
    client: HarnessClient,
    prompt: RwSignal<String>,
    auth_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let run_state = move || client.status.get().run_state;

    let can_start = move || {
        client.config_valid.get_untracked()
            && matches!(
                run_state(),
                RunState::Idle | RunState::Finished | RunState::Failed | RunState::Stopped
            )
    };

    let can_stop = move || matches!(run_state(), RunState::Running | RunState::WaitingInput);

    let on_start = move |_| {
        let text = prompt.get_untracked().trim().to_string();
        if text.is_empty() {
            return;
        }

        spawn_local({
            let client = client;
            let auth_error = auth_error;
            async move {
                match client.gateway().start_run(text).await {
                    Ok(response) => {
                        client.run_url.set(Some(response.run_url));
                        client.status.update(|status| {
                            status.run_state = RunState::Running;
                            status.started_at = None;
                            status.updated_at = None;
                            status.summary = None;
                            status.pending_input = None;
                            status.result = None;
                        });
                        client.connected.set(true);
                        client.consecutive_errors.set(0);
                    }
                    Err(GatewayError::Http { status: 401, .. }) => {
                        clear_token();
                        client.auth_token.set(None);
                        client.connected.set(false);
                        auth_error.set(Some("Неверный токен.".into()));
                    }
                    Err(_) => {
                        client.connected.set(false);
                    }
                }
            }
        });
    };

    let on_stop = move |_| {
        spawn_local({
            let client = client;
            let auth_error = auth_error;
            async move {
                match client.gateway().stop_run("остановлено пользователем").await {
                    Ok(response) => {
                        client.status.update(|status| {
                            status.run_state = response.run_state;
                        });
                        client.connected.set(true);
                    }
                    Err(GatewayError::Http { status: 401, .. }) => {
                        clear_token();
                        client.auth_token.set(None);
                        client.connected.set(false);
                        auth_error.set(Some("Неверный токен.".into()));
                    }
                    Err(_) => {
                        client.connected.set(false);
                    }
                }
            }
        });
    };

    view! {
        <div class="hw-composer">
            <label class="hw-field">
                <span>"Промпт"</span>
                <textarea
                    class="hw-input hw-monospace"
                    rows="6"
                    prop:value=move || prompt.get()
                    on:input=move |ev| prompt.set(event_target_value(&ev))
                    placeholder="Что нужно сделать?"
                ></textarea>
            </label>
            <div class="hw-actions">
                <button type="button" class="hw-btn" on:click=on_start disabled=move || !can_start()>
                    "Start"
                </button>
                <button type="button" class="hw-btn" on:click=on_stop disabled=move || !can_stop()>
                    "Stop"
                </button>
            </div>
        </div>
    }
}
