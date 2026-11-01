//! Thin standalone entry point for the Trunk build.

use leptos::prelude::*;
use web_harness::HarnessApp;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <HarnessApp/> });
}
