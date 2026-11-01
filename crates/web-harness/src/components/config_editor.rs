//! Configuration editor for `fs-panel` builds.
//!
//! Loads the redacted `harness.json5` through the config API, saves it with
//! `PUT /config`, and offers `POST /config/default` when the file is missing.
//! Secret values are shown as `"***"` and saving `"***"` preserves the server
//! value thanks to the server-side merge rule.

use crate::components::connect_dialog::clear_token;
use crate::gateway::GatewayError;
use crate::state::HarnessClient;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Configuration editor component.
#[component]
pub fn ConfigEditor(client: HarnessClient) -> impl IntoView {
    let text = RwSignal::new(String::new());
    let exists = RwSignal::new(false);
    let valid = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);

    let load = move || {
        spawn_local({
            let client = client;
            let text = text;
            let exists = exists;
            let valid = valid;
            let error = error;
            let loaded = loaded;
            async move {
                match client.gateway().get_config().await {
                    Ok(response) => {
                        text.set(response.text.unwrap_or_default());
                        exists.set(response.exists);
                        valid.set(response.valid);
                        client.config_valid.set(response.valid);
                        error.set(response.validation_error);
                        loaded.set(true);
                    }
                    Err(GatewayError::Http { status: 401, .. }) => {
                        clear_token();
                        client.auth_token.set(None);
                        client.connected.set(false);
                        loaded.set(true);
                    }
                    Err(err) => {
                        error.set(Some(config_error_text(&err)));
                        loaded.set(true);
                    }
                }
            }
        });
    };

    load();

    let save = move |_| {
        spawn_local({
            let client = client;
            let text = text;
            let valid = valid;
            let error = error;
            async move {
                match client.gateway().put_config(text.get_untracked()).await {
                    Ok(response) if response.ok => {
                        valid.set(true);
                        client.config_valid.set(true);
                        error.set(None);
                    }
                    Ok(response) => {
                        valid.set(false);
                        client.config_valid.set(false);
                        error.set(response.error.map(|err| err.message));
                    }
                    Err(GatewayError::Http {
                        code, message, ..
                    }) => {
                        valid.set(false);
                        client.config_valid.set(false);
                        error.set(Some(format!("{code}: {message}")));
                    }
                    Err(err) => {
                        valid.set(false);
                        client.config_valid.set(false);
                        error.set(Some(config_error_text(&err)));
                    }
                }
            }
        });
    };

    let create_default = move |_| {
        spawn_local({
            let client = client;
            let text = text;
            let exists = exists;
            let valid = valid;
            let error = error;
            let loaded = loaded;
            async move {
                match client.gateway().config_default().await {
                    Ok(response) => {
                        text.set(response.text);
                        exists.set(response.exists);
                        valid.set(response.valid);
                        client.config_valid.set(response.valid);
                        error.set(None);
                        loaded.set(true);
                    }
                    Err(err) => {
                        error.set(Some(config_error_text(&err)));
                        loaded.set(true);
                    }
                }
            }
        });
    };

    view! {
        <div class="hw-config">
            <h3>"Конфигурация (harness.json5)"</h3>
            <Show when=move || loaded.get() fallback=|| view! { <p>"Загрузка..."</p> }>
                <div class="hw-config-editor">
                    <textarea
                        class="hw-input hw-monospace"
                        rows="20"
                        prop:value=move || text.get()
                        on:input=move |ev| text.set(event_target_value(&ev))
                    ></textarea>
                    <div class="hw-actions">
                        <button type="button" class="hw-btn" on:click=save>
                            "Сохранить"
                        </button>
                        <Show when=move || !exists.get() fallback=|| view! {}>
                            <button type="button" class="hw-btn" on:click=create_default>
                                "Создать по умолчанию"
                            </button>
                        </Show>
                    </div>
                    <Show when=move || error.get().is_some() fallback=|| view! {}>
                        <p class="hw-error" role="alert">
                            {move || error.get().unwrap_or_default()}
                        </p>
                    </Show>
                    <p class="hw-hint">
                        "Секретные поля отображаются как \"***\". Сохранение \"***\" не затирает реальное значение."
                    </p>
                </div>
            </Show>
        </div>
    }
}

fn config_error_text(err: &GatewayError) -> String {
    match err {
        GatewayError::Timeout => "Сервер не ответил за 10 секунд.".into(),
        GatewayError::Http {
            code, message, ..
        } => format!("{code}: {message}"),
        GatewayError::Network(_) => "Сеть недоступна.".into(),
        GatewayError::Decode(_) => "Некорректный ответ сервера.".into(),
    }
}
