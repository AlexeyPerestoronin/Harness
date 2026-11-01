//! `web-harness` — Leptos CSR client for `harness.exe --serve`.
//!
//! The crate can be used in two ways:
//!
//! 1. As a standalone Trunk application (`src/main.rs` mounts [`HarnessApp`]).
//! 2. As a library that exports [`HarnessGateway`], [`HarnessClient`] and the
//!    embeddable [`HarnessPanel`] component.
//!
//! The client talks to the server only through HTTPS REST and fixed-interval
//! polling. It intentionally knows nothing about agents, API keys or session
//! logs: those arrive from the server as ordinary `pendingInput` requests.

mod gateway;
mod state;

#[cfg(target_arch = "wasm32")]
mod components;

pub use gateway::{
    classify_status, is_terminal_run_state, GatewayError, HarnessGateway, PollAction,
    StopRunResponse, POLL_INTERVAL,
};
pub use state::HarnessClient;

#[cfg(target_arch = "wasm32")]
pub use components::harness_app::HarnessApp;
#[cfg(target_arch = "wasm32")]
pub use components::harness_panel::{HarnessPanel, HarnessTheme};
