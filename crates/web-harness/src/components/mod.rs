//! Leptos UI components of the web client.
//!
//! The module tree mirrors the standalone/embeddable panel layout. The
//! `fs-panel` feature enables config and file-browser components for host
//! applications.

pub mod connect_dialog;
pub mod harness_app;
pub mod harness_panel;
pub mod input_modal;
pub mod prompt_composer;
pub mod result_view;
pub mod status_bar;

#[cfg(feature = "fs-panel")]
pub mod config_editor;
#[cfg(feature = "fs-panel")]
pub mod fs_browser;
