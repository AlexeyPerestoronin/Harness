//! Modal dialog for pending user-input requests.
//!
//! Any `pendingInput` is rendered by the same modal: `text` requests show an
//! input/textarea (password-masked when `sensitive` is set) and `confirm`
//! requests show Yes/No buttons. Escape closes the modal with confirmation
//! when there is an unsaved answer.

use crate::components::connect_dialog::clear_token;
use crate::gateway::GatewayError;
use crate::state::HarnessClient;
use harness_api::PendingInputKind;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Input modal component.
#[component]
pub fn InputModal(
    client: HarnessClient,
    auth_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let answer = RwSignal::new(String::new());

    let pending_input = move || client.status.get().pending_input.clone();

    let close = move || {
        client.modal_open.set(false);
        answer.set(String::new());
    };

    let submit = move || {
        let Some(pending) = pending_input() else {
            return;
        };

        let answer_value = match pending.kind {
            PendingInputKind::Confirm => answer.get_untracked(),
            PendingInputKind::Text => answer.get_untracked().trim().to_string(),
        };

        if answer_value.is_empty() {
            return;
        }

        spawn_local({
            let client = client;
            let auth_error = auth_error;
            let answer = answer;
            async move {
                match client
                    .gateway()
                    .submit_input(pending.request_id.clone(), answer_value)
                    .await
                {
                    Ok(_) => {
                        client.status.update(|status| {
                            status.pending_input = None;
                        });
                        client.modal_open.set(false);
                        answer.set(String::new());
                    }
                    Err(GatewayError::Http { status: 401, .. }) => {
                        clear_token();
                        client.auth_token.set(None);
                        client.connected.set(false);
                        client.modal_open.set(false);
                        auth_error.set(Some("Неверный токен.".into()));
                    }
                    Err(_) => {
                        // Keep the modal open; the next polling cycle will retry.
                    }
                }
            }
        });
    };

    window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" && client.modal_open.get_untracked() {
            let unsaved = !answer.get_untracked().is_empty();
            if !unsaved
                || web_sys::window()
                    .and_then(|window| {
                        window
                            .confirm_with_message("Закрыть без отправки ответа?")
                            .ok()
                    })
                    .unwrap_or(false)
            {
                close();
            }
        }
    });

    view! {
        <Show when=move || client.modal_open.get() && pending_input().is_some() fallback=|| view! {}>
            <div class="hw-modal-backdrop" role="dialog" aria-modal="true">
                <div class="hw-modal">
                    <h3>"Требуется ответ"</h3>
                    <p class="hw-prompt">
                        {move || pending_input().map(|pending| pending.prompt.clone()).unwrap_or_default()}
                    </p>

                    {move || {
                        let Some(pending) = pending_input() else {
                            return view! {}.into_any();
                        };

                        match pending.kind {
                            PendingInputKind::Confirm => view! {
                                <div class="hw-actions">
                                    <button
                                        type="button"
                                        class="hw-btn"
                                        on:click=move |_| {
                                            answer.set("y".into());
                                            submit();
                                        }
                                    >
                                        "Yes"
                                    </button>
                                    <button
                                        type="button"
                                        class="hw-btn"
                                        on:click=move |_| {
                                            answer.set("n".into());
                                            submit();
                                        }
                                    >
                                        "No"
                                    </button>
                                </div>
                            }.into_any(),
                            PendingInputKind::Text if pending.sensitive => view! {
                                <div class="hw-field">
                                    <input
                                        type="password"
                                        class="hw-input"
                                        prop:value=move || answer.get()
                                        on:input=move |ev| answer.set(event_target_value(&ev))
                                    />
                                </div>
                                <div class="hw-actions">
                                    <button type="button" class="hw-btn" on:click=move |_| submit()>
                                        "Ответить"
                                    </button>
                                </div>
                            }.into_any(),
                            PendingInputKind::Text => view! {
                                <div class="hw-field">
                                    <textarea
                                        class="hw-input hw-monospace"
                                        rows="4"
                                        prop:value=move || answer.get()
                                        on:input=move |ev| answer.set(event_target_value(&ev))
                                    ></textarea>
                                </div>
                                <div class="hw-actions">
                                    <button type="button" class="hw-btn" on:click=move |_| submit()>
                                        "Ответить"
                                    </button>
                                </div>
                            }.into_any(),
                        }
                    }}
                </div>
            </div>
        </Show>
    }
}
