//! Terminal run result view.
//!
//! Shows `finished`/`failed`/`stopped`, final counters, and the error text for
//! unsuccessful runs. The "new run" action resets the client and re-enables
//! Start.

use crate::state::HarnessClient;
use harness_api::RunState;
use leptos::prelude::*;

/// Result view component.
#[component]
pub fn ResultView(client: HarnessClient) -> impl IntoView {
    let new_run = move |_| {
        client.reset();
    };

    let result_view = move || {
        let Some(result) = client.status.get().result.clone() else {
            return view! {}.into_any();
        };

        let state_label = match result.run_state {
            RunState::Finished => "Завершено",
            RunState::Failed => "Ошибка",
            RunState::Stopped => "Остановлено",
            _ => "Завершено",
        };

        let error = if result.ok {
            None
        } else {
            result.error.clone()
        };

        view! {
            <div class="hw-result" role="status">
                <h3>"Результат"</h3>
                <p><strong>{state_label}</strong></p>
                <ul class="hw-stats">
                    <li>{format!("Итерации: {}", result.stats.iteration)}</li>
                    <li>{format!("Токены: {}", result.stats.tokens)}</li>
                    <li>{format!("USD (прибл.): ${:.4}", result.stats.usd_approx)}</li>
                    {match result.stats.usd_real {
                        Some(real) => view! { <li>{format!("USD (real): ${real:.4}")}</li> }.into_any(),
                        None => view! {}.into_any(),
                    }}
                </ul>
                {match error {
                    Some(message) => view! { <p class="hw-error">{message}</p> }.into_any(),
                    None => view! {}.into_any(),
                }}
                <button type="button" class="hw-btn" on:click=new_run>
                    "Новый запуск"
                </button>
            </div>
        }
        .into_any()
    };

    view! {
        <Show when=move || client.status.get().result.is_some() fallback=|| view! {}>
            {result_view}
        </Show>
    }
}
