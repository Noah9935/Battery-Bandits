//! Weboberfläche von Battery Bandits: Leptos (Client-seitig gerendert, WebAssembly).

mod api;
mod app;
mod chart;
mod drawer;
mod filter;
mod format;
mod inputs;
mod jsutil;
mod maintenance;
mod map_view;
mod markers;
mod overview;
mod sse;
mod state;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
