//! Standalone Leptos application wrapper.
//!
//! Reads the optional `?server=` query parameter and delegates the whole UI
//! to [`HarnessPanel`].

use crate::components::connect_dialog::server_from_query;
use crate::components::harness_panel::HarnessPanel;
use leptos::prelude::*;

/// Standalone entry component mounted by `main.rs`.
#[component]
pub fn HarnessApp() -> impl IntoView {
    let base_url =
        server_from_query().unwrap_or_else(|| "https://localhost:4067".to_string());

    view! { <HarnessPanel base_url=base_url compact=false /> }
}
