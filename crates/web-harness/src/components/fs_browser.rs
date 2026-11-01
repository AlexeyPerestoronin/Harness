//! File-system browser for `fs-panel` builds.
//!
//! Operates through the FS REST API: list, read, write, mkdir, remove and
//! move. Destructive actions ask for confirmation. Deny-listed paths are not
//! filtered client-side; the server responds with `403`, which is shown as an
//! inaccessible-path error.

use crate::gateway::GatewayError;
use crate::state::HarnessClient;
use harness_api::FsEntry;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

fn join_fs_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_string(),
        None => String::new(),
    }
}

fn prompt(message: &str, default: &str) -> Option<String> {
    web_sys::window()?
        .prompt_with_message_and_default(message, default)
        .ok()?
}

fn confirm(message: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.confirm_with_message(message).ok())
        .unwrap_or(false)
}

fn fs_error_text(err: &GatewayError) -> String {
    match err {
        GatewayError::Http {
            status: 403,
            message,
            ..
        } => format!("Доступ запрещён: {message}"),
        GatewayError::Http {
            status, message, ..
        } => format!("Ошибка {status}: {message}"),
        GatewayError::Timeout => "Сервер не ответил за 10 секунд.".into(),
        GatewayError::Network(_) => "Сеть недоступна.".into(),
        GatewayError::Decode(_) => "Некорректный ответ сервера.".into(),
    }
}

/// File-system browser component.
#[component]
pub fn FsBrowser(client: HarnessClient) -> impl IntoView {
    let current = RwSignal::new(String::new());
    let entries = RwSignal::new(Vec::<FsEntry>::new());
    let selected_name = RwSignal::new(None::<String>);
    let file_text = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);

    let reload = move || {
        let path = current.get_untracked();
        spawn_local({
            let client = client;
            let entries = entries;
            let error = error;
            let path = path.clone();
            async move {
                match client.gateway().fs_list(&path).await {
                    Ok(response) => {
                        entries.set(response.entries);
                        error.set(None);
                    }
                    Err(err) => {
                        entries.set(Vec::new());
                        error.set(Some(fs_error_text(&err)));
                    }
                }
            }
        });
    };

    reload();

    let open_entry = move |name: String, kind: String| {
        if kind == "dir" {
            current.set(join_fs_path(&current.get_untracked(), &name));
            reload();
            return;
        }

        let path = join_fs_path(&current.get_untracked(), &name);
        selected_name.set(Some(name));
        spawn_local({
            let client = client;
            let file_text = file_text;
            let error = error;
            let path = path.clone();
            async move {
                match client.gateway().fs_read(&path).await {
                    Ok(response) => {
                        file_text.set(response.text);
                        error.set(None);
                    }
                    Err(err) => {
                        file_text.set(String::new());
                        error.set(Some(fs_error_text(&err)));
                    }
                }
            }
        });
    };

    let go_up = move |_| {
        current.set(parent_path(&current.get_untracked()));
        reload();
    };

    let create_dir = move |_| {
        let Some(name) = prompt("Имя каталога:", "") else {
            return;
        };
        let path = join_fs_path(&current.get_untracked(), name.trim());
        if path.is_empty() {
            return;
        }
        spawn_local({
            let client = client;
            let error = error;
            async move {
                if let Err(err) = client.gateway().fs_mkdir(&path).await {
                    error.set(Some(fs_error_text(&err)));
                }
                reload();
            }
        });
    };

    let create_file = move |_| {
        let Some(name) = prompt("Имя файла:", "") else {
            return;
        };
        let name = name.trim().to_string();
        let path = join_fs_path(&current.get_untracked(), &name);
        if name.is_empty() || path.is_empty() {
            return;
        }
        spawn_local({
            let client = client;
            let selected_name = selected_name;
            let file_text = file_text;
            let error = error;
            async move {
                if let Err(err) = client.gateway().fs_write(&path, "").await {
                    error.set(Some(fs_error_text(&err)));
                } else {
                    selected_name.set(Some(name));
                    file_text.set(String::new());
                    error.set(None);
                }
                reload();
            }
        });
    };

    let save_file = move |_| {
        let Some(name) = selected_name.get_untracked() else {
            return;
        };
        let path = join_fs_path(&current.get_untracked(), &name);
        if !confirm(&format!("Перезаписать/сохранить файл {name}?")) {
            return;
        }
        spawn_local({
            let client = client;
            let error = error;
            async move {
                if let Err(err) = client.gateway().fs_write(&path, file_text.get_untracked()).await {
                    error.set(Some(fs_error_text(&err)));
                } else {
                    error.set(None);
                }
                reload();
            }
        });
    };

    let delete_file = move |_| {
        let Some(name) = selected_name.get_untracked() else {
            return;
        };
        let path = join_fs_path(&current.get_untracked(), &name);
        if !confirm(&format!("Удалить {name}?")) {
            return;
        }
        spawn_local({
            let client = client;
            let selected_name = selected_name;
            let file_text = file_text;
            let error = error;
            async move {
                if let Err(err) = client.gateway().fs_remove(&path).await {
                    error.set(Some(fs_error_text(&err)));
                } else {
                    selected_name.set(None);
                    file_text.set(String::new());
                    error.set(None);
                }
                reload();
            }
        });
    };

    let move_file = move |_| {
        let Some(name) = selected_name.get_untracked() else {
            return;
        };
        let src = join_fs_path(&current.get_untracked(), &name);
        let Some(dst) = prompt("Новый путь (относительно cwd):", &src) else {
            return;
        };
        if dst.trim().is_empty() {
            return;
        }
        spawn_local({
            let client = client;
            let error = error;
            let selected_name = selected_name;
            let file_text = file_text;
            async move {
                if let Err(err) = client.gateway().fs_move(&src, dst.trim()).await {
                    error.set(Some(fs_error_text(&err)));
                } else {
                    selected_name.set(None);
                    file_text.set(String::new());
                    error.set(None);
                }
                reload();
            }
        });
    };

    view! {
        <div class="hw-fs">
            <div class="hw-fs-toolbar">
                <button type="button" class="hw-btn" on:click=go_up>"Назад"</button>
                <span class="hw-path">
                    {move || if current.get().is_empty() { "cwd".to_string() } else { current.get() }}
                </span>
                <button type="button" class="hw-btn" on:click=create_dir>"Создать каталог"</button>
                <button type="button" class="hw-btn" on:click=create_file>"Создать файл"</button>
            </div>

            <ul class="hw-fs-list">
                {move || entries.get().iter().map(|entry| {
                    let name = entry.name.clone();
                    let kind = entry.kind.clone();
                    let display = format!("{} {}", if entry.kind == "dir" { "[D]" } else { "[F]" }, entry.name);
                    view! {
                        <li>
                            <button
                                type="button"
                                class="hw-fs-entry"
                                on:click=move |_| open_entry(name.clone(), kind.clone())
                            >
                                {display}
                            </button>
                        </li>
                    }.into_any()
                }).collect::<Vec<_>>()}
            </ul>

            <Show when=move || selected_name.get().is_some() fallback=|| view! {}>
                <div class="hw-fs-editor">
                    <h4>{move || selected_name.get().unwrap_or_default()}</h4>
                    <textarea
                        class="hw-input hw-monospace"
                        rows="16"
                        prop:value=move || file_text.get()
                        on:input=move |ev| file_text.set(event_target_value(&ev))
                    ></textarea>
                    <div class="hw-actions">
                        <button type="button" class="hw-btn" on:click=save_file>"Сохранить"</button>
                        <button type="button" class="hw-btn" on:click=delete_file>"Удалить"</button>
                        <button type="button" class="hw-btn" on:click=move_file>"Переименовать/переместить"</button>
                    </div>
                </div>
            </Show>

            <Show when=move || error.get().is_some() fallback=|| view! {}>
                <p class="hw-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
        </div>
    }
}
