//! Weboberfläche von Battery Bandits: Leptos (Client-seitig gerendert, WebAssembly).

mod api;
mod app;
mod chart;
mod drawer;
mod features;
mod filter;
mod format;
mod icons;
mod inputs;
mod insights;
mod jsutil;
mod maintenance;
mod map_view;
mod markers;
mod overlays;
mod overview;
mod search;
mod share;
mod sse;
mod state;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
