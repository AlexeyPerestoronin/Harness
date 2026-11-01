//! Server connection dialog.
//!
//! The dialog uses a two-step flow: first `GET /health` is checked without a
//! token, then the token is validated against a protected endpoint. The token
//! is stored only in the current origin's `localStorage`; it is never passed
//! through query parameters.

use crate::gateway::{GatewayError, HarnessGateway};
use crate::state::HarnessClient;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

const SERVER_STORAGE_KEY: &str = "harness.server";
const TOKEN_STORAGE_KEY: &str = "harness.token";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub(crate) fn load_server() -> Option<String> {
    storage()?.get_item(SERVER_STORAGE_KEY).ok()?
}

pub(crate) fn load_token() -> Option<String> {
    storage()?.get_item(TOKEN_STORAGE_KEY).ok()?
}

pub(crate) fn save_server(server: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(SERVER_STORAGE_KEY, server);
    }
}

pub(crate) fn save_token(token: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(TOKEN_STORAGE_KEY, token);
    }
}

pub(crate) fn clear_token() {
    if let Some(storage) = storage() {
        let _ = storage.remove_item(TOKEN_STORAGE_KEY);
    }
}

/// Reads the optional `?server=` query parameter.
pub(crate) fn server_from_query() -> Option<String> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
    params.get("server")
}

fn connection_error_message(err: &GatewayError) -> String {
    match err {
        GatewayError::Timeout => "Сервер не ответил за 10 секунд.".into(),
        GatewayError::Http {
            code, message, ..
        } => format!("Ошибка сервера {code}: {message}"),
        GatewayError::Network(_) => {
            "Сервер не найден или CORS запрещён. См. инструкцию ниже.".into()
        }
        GatewayError::Decode(_) => "Некорректный ответ сервера.".into(),
    }
}

/// Connection dialog component.
#[component]
pub fn ConnectDialog(
    client: HarnessClient,
    auth_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let server = RwSignal::new(client.base_url.get_untracked());
    let token = RwSignal::new(String::new());
    let health_checked = RwSignal::new(false);
    let auth_required = RwSignal::new(false);
    let busy = RwSignal::new(false);

    let check_server = move |_| {
        let gateway = HarnessGateway::new(server.get_untracked(), None);
        busy.set(true);
        spawn_local(async move {
            match gateway.health().await {
                Ok(health) => {
                    client.base_url.set(gateway.base_url().to_string());
                    save_server(gateway.base_url());
                    auth_required.set(health.auth_required);
                    health_checked.set(true);
                    auth_error.set(None);
                    if !health.auth_required {
                        client.auth_token.set(None);
                        client.connected.set(true);
                    }
                }
                Err(err) => {
                    auth_error.set(Some(connection_error_message(&err)));
                }
            }
            busy.set(false);
        });
    };

    let connect = move |_| {
        let token_value = token.get_untracked().trim().to_string();
        if token_value.is_empty() {
            auth_error.set(Some("Введите токен.".into()));
            return;
        }

        client.auth_token.set(Some(token_value.clone()));
        client.base_url.set(server.get_untracked());
        let gateway = HarnessGateway::new(server.get_untracked(), Some(token_value));

        busy.set(true);
        spawn_local(async move {
            match gateway.get_run_status("/api/v1/run/current").await {
                Ok(status) => {
                    client.status.set(status);
                    client.connected.set(true);
                    client.consecutive_errors.set(0);
                    save_server(gateway.base_url());
                    if let Some(token) = gateway.auth_token() {
                        save_token(token);
                    }
                    auth_error.set(None);
                }
                Err(GatewayError::Http { status: 401, .. }) => {
                    clear_token();
                    client.auth_token.set(None);
                    auth_error.set(Some(
                        "Неверный токен. Проверьте токен из stdout сервера.".into(),
                    ));
                }
                Err(err) => {
                    client.auth_token.set(None);
                    auth_error.set(Some(connection_error_message(&err)));
                }
            }
            busy.set(false);
        });
    };

    view! {
        <div class="hw-connect">
            <h2>"Подключение к серверу"</h2>

            <label class="hw-field">
                <span>"Адрес сервера"</span>
                <input
                    type="text"
                    class="hw-input"
                    prop:value=move || server.get()
                    on:input=move |ev| server.set(event_target_value(&ev))
                    placeholder="https://localhost:4067"
                />
            </label>
            <button type="button" class="hw-btn" on:click=check_server disabled=move || busy.get()>
                "Проверить"
            </button>

            <Show when=move || health_checked.get() && auth_required.get() fallback=|| view! {}>
                <label class="hw-field">
                    <span>"Bearer-токен"</span>
                    <input
                        type="password"
                        class="hw-input"
                        prop:value=move || token.get()
                        on:input=move |ev| token.set(event_target_value(&ev))
                        placeholder="Токен из stdout harness.exe"
                    />
                </label>
                <button type="button" class="hw-btn" on:click=connect disabled=move || busy.get()>
                    "Подключиться"
                </button>
            </Show>

            <Show when=move || auth_error.get().is_some() fallback=|| view! {}>
                <p class="hw-error" role="alert">
                    {move || auth_error.get().unwrap_or_default()}
                </p>
            </Show>

            <details class="hw-help">
                <summary>"Сервер не найден или CORS запрещён?"</summary>
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
            </details>
        </div>
    }
}
