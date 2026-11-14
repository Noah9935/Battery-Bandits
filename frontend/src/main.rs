//! Weboberfläche von Battery Bandits: Leptos (Client-seitig gerendert, WebAssembly).

mod admin;
mod api;
mod app;
mod avatar;
mod batteries;
mod chart;
mod device;
mod efficiency_view;
mod features;
mod filter;
mod format;
mod forward;
mod gesture;
mod history;
mod icons;
mod inbox;
mod inputs;
mod insights;
mod jsutil;
mod limit;
mod login;
mod maintenance;
mod map_view;
mod markers;
mod nodes;
mod overlays;
mod overview;
mod pager;
mod pins;
mod profile;
mod retire;
mod route;
mod routing;
mod search;
mod share;
mod sse;
mod state;
mod swap;
mod swipe;
mod tickets;
mod tour;
mod tour_plan;
mod urgency;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
