//! Embeddable harness control panel.
//!
//! The panel owns connection state, the 10-second polling loop, and layout
//! for all base components. With the `fs-panel` feature it additionally
//! renders the configuration and file-browser tabs.

use crate::components::connect_dialog::{
    clear_token, load_server, load_token, save_server, ConnectDialog,
};
use crate::components::input_modal::InputModal;
use crate::components::prompt_composer::PromptComposer;
use crate::components::result_view::ResultView;
use crate::components::status_bar::StatusBar;
use crate::gateway::{
    classify_status, is_terminal_run_state, GatewayError, PollAction,
};
use crate::state::HarnessClient;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[cfg(feature = "fs-panel")]
use crate::components::{config_editor::ConfigEditor, fs_browser::FsBrowser};

/// Visual theme of the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HarnessTheme {
    /// Light theme.
    #[default]
    Light,
    /// Dark theme.
    Dark,
}

/// Polls the current run once and updates reactive client state.
pub(crate) async fn poll_once(
    client: HarnessClient,
    server_lost: RwSignal<bool>,
    auth_error: RwSignal<Option<String>>,
) {
    let Some(run_url) = client.run_url.get_untracked() else {
        return;
    };

    let gateway = client.gateway();
    match gateway.get_run_status(&run_url).await {
        Ok(status) => {
            client.connected.set(true);
            client.consecutive_errors.set(0);
            server_lost.set(false);
            client.status.set(status.clone());

            match classify_status(&status) {
                PollAction::ShowInput => client.modal_open.set(true),
                PollAction::ShowResult => client.modal_open.set(false),
                PollAction::Continue => {}
            }
        }
        Err(GatewayError::Http { status: 401, .. }) => {
            clear_token();
            client.auth_token.set(None);
            client.connected.set(false);
            auth_error.set(Some("Неверный токен.".into()));
        }
        Err(_) => {
            client.connected.set(false);
            let count = client.consecutive_errors.get_untracked() + 1;
            client.consecutive_errors.set(count);
            if count >= 6 {
                server_lost.set(true);
            }
        }
    }
}

#[cfg(not(feature = "fs-panel"))]
fn panel_body(
    client: HarnessClient,
    prompt: RwSignal<String>,
    auth_error: RwSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <StatusBar client=client />
        <div class="hw-main">
            <section class="hw-composer-col">
                <PromptComposer client=client prompt=prompt auth_error=auth_error />
            </section>
            <section class="hw-result-col">
                <ResultView client=client />
            </section>
        </div>
        <InputModal client=client auth_error=auth_error />
    }
}

#[cfg(feature = "fs-panel")]
fn panel_body(
    client: HarnessClient,
    prompt: RwSignal<String>,
    auth_error: RwSignal<Option<String>>,
    active_tab: RwSignal<u8>,
) -> impl IntoView {
    view! {
        <StatusBar client=client />
        <div class="hw-tabs" role="tablist">
            <button
                type="button"
                class="hw-tab"
                class:active=move || active_tab.get() == 0
                on:click=move |_| active_tab.set(0)
            >
                "Цикл"
            </button>
            <button
                type="button"
                class="hw-tab"
                class:active=move || active_tab.get() == 1
                on:click=move |_| active_tab.set(1)
            >
                "Конфигурация"
            </button>
            <button
                type="button"
                class="hw-tab"
                class:active=move || active_tab.get() == 2
                on:click=move |_| active_tab.set(2)
            >
                "Файлы"
            </button>
        </div>

        <Show when=move || active_tab.get() == 0 fallback=|| view! {}>
            <div class="hw-main">
                <section class="hw-composer-col">
                    <PromptComposer client=client prompt=prompt auth_error=auth_error />
                </section>
                <section class="hw-result-col">
                    <ResultView client=client />
                </section>
            </div>
        </Show>

        <Show when=move || active_tab.get() == 1 fallback=|| view! {}>
            <ConfigEditor client=client />
        </Show>

        <Show when=move || active_tab.get() == 2 fallback=|| view! {}>
            <FsBrowser client=client />
        </Show>

        <InputModal client=client auth_error=auth_error />
    }
}

/// Embeddable panel component.
#[component]
pub fn HarnessPanel(
    base_url: String,
    #[prop(optional)] auth_token: Option<String>,
    #[prop(optional)] compact: bool,
    #[prop(optional)] theme: Option<HarnessTheme>,
) -> impl IntoView {
    let initial_base = if base_url.trim().is_empty() {
        crate::components::connect_dialog::server_from_query()
            .or_else(load_server)
            .unwrap_or_else(|| "https://localhost:4067".to_string())
    } else {
        base_url
    };

    let client = HarnessClient::new(initial_base, auth_token);
    let auth_error = RwSignal::new(None::<String>);
    let server_lost = RwSignal::new(false);
    let prompt = RwSignal::new(String::new());
    #[cfg(feature = "fs-panel")]
    let active_tab = RwSignal::new(0u8);
    #[cfg(not(feature = "fs-panel"))]
    let _ = ();
    let _theme = theme.unwrap_or_default();

    // Try to connect automatically when a token is available from props or storage.
    spawn_local({
        let client = client;
        let auth_error = auth_error;
        async move {
            let token = match client.auth_token.get_untracked() {
                Some(token) => Some(token),
                None => load_token(),
            };

            if let Some(token) = token {
                client.auth_token.set(Some(token.clone()));
                match client.gateway().get_run_status("/api/v1/run/current").await {
                    Ok(status) => {
                        client.status.set(status);
                        client.connected.set(true);
                        client.consecutive_errors.set(0);
                        save_server(client.base_url.get_untracked().as_str());
                    }
                    Err(GatewayError::Http { status: 401, .. }) => {
                        clear_token();
                        client.auth_token.set(None);
                        auth_error.set(Some(
                            "Неверный токен. Введите новый токен.".into(),
                        ));
                    }
                    Err(_) => {
                        client.connected.set(false);
                    }
                }
            }
        }
    });

    // Background polling loop: once a run URL is known, poll every 10 seconds
    // until a terminal state or until run_url is cleared.
    spawn_local({
        let client = client;
        let server_lost = server_lost;
        let auth_error = auth_error;
        async move {
            loop {
                TimeoutFuture::new(10_000).await;
                if client.run_url.get_untracked().is_none() {
                    continue;
                }
                if is_terminal_run_state(client.status.get_untracked().run_state) {
                    continue;
                }
                poll_once(client, server_lost, auth_error).await;
            }
        }
    });

    let panel_class = move || {
        if compact {
            "hw-panel hw-compact".to_string()
        } else {
            "hw-panel".to_string()
        }
    };

    #[cfg(not(feature = "fs-panel"))]
    let fallback = move || panel_body(client, prompt, auth_error);
    #[cfg(feature = "fs-panel")]
    let fallback = move || panel_body(client, prompt, auth_error, active_tab);

    view! {
        <div class=panel_class>
            <Show
                when=move || !client.connected.get()
                fallback=fallback
            >
                <ConnectDialog client=client auth_error=auth_error />
            </Show>

            <Show when=move || server_lost.get() fallback=|| view! {}>
                <ServerLostModal client=client server_lost=server_lost auth_error=auth_error />
            </Show>
        </div>
    }
}

/// Modal shown after six consecutive polling failures.
#[component]
fn ServerLostModal(
    client: HarnessClient,
    server_lost: RwSignal<bool>,
    auth_error: RwSignal<Option<String>>,
) -> impl IntoView {
    window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" && server_lost.get_untracked() {
            server_lost.set(false);
        }
    });

    let retry = move |_| {
        client.consecutive_errors.set(0);
        server_lost.set(false);
        spawn_local({
            let client = client;
            let server_lost = server_lost;
            let auth_error = auth_error;
            async move {
                poll_once(client, server_lost, auth_error).await;
            }
        });
    };

    let copy_command = move |_| {
        if let Some(window) = web_sys::window() {
            let _ = window.navigator().clipboard().write_text("harness.exe");
        }
    };

    view! {
        <div class="hw-modal-backdrop" role="dialog" aria-modal="true">
            <div class="hw-modal">
                <h3>"Сервер не найден / CORS запрещён"</h3>
                <ol>
                    <li>"Скачайте harness.exe с GitHub Releases."</li>
                    <li>"Запустите его без параметров: <code>harness.exe</code>"</li>
                    <li>
                        "Примите самоподписанный сертификат в браузере: откройте "
                        <code>"https://localhost:4067/api/v1/health"</code>
                        " и подтвердите исключение."
                    </li>
                    <li>
                        "Если клиент и сервер на разных origin, добавьте origin клиента в "
                        <code>"server.cors-allowed-origins"</code>
                        " в harness.json5."
                    </li>
                </ol>
                <div class="hw-actions">
                    <button type="button" class="hw-btn" on:click=retry>
                        "Повторить подключение"
                    </button>
                    <button type="button" class="hw-btn" on:click=copy_command>
                        "Скопировать команду запуска"
                    </button>
                </div>
            </div>
        </div>
    }
}
