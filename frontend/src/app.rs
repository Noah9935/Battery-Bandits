//! Rahmen der Anwendung: Seitenleiste (auf dem Handy untere Leiste), Hinweise, Seitenwahl.

use leptos::prelude::*;

use crate::drawer::Drawer;
use crate::icons::Icon;
use crate::jsutil;
use crate::maintenance::Maintenance;
use crate::map_view::MapView;
use crate::overview::Overview;
use crate::state::{Page, Store, start};

/// Marke: der Waschbär mit eckigen Konturen (Datei unter assets/logo, von Trunk nach dist kopiert).
#[component]
pub fn Logo() -> impl IntoView {
    view! { <img class="logo" src="battery-bandits-logo.svg" alt="" width="32" height="32"/> }
}

fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Overview => "layout-dashboard",
        Page::Map => "map-pin",
        Page::Maintenance => "wrench",
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let s = expect_context::<Store>();
    let dark = RwSignal::new(jsutil::is_dark());
    view! {
        <nav class="side" aria-label="Hauptnavigation">
            <div class="brand">
                <Logo/>
                <span class="brand-name">"Battery Bandits"</span>
            </div>
            <ul class="nav">
                {Page::ALL
                    .into_iter()
                    .map(|page| {
                        view! {
                            <li>
                                <button
                                    class=move || if s.page.get() == page { "nav-item active" } else { "nav-item" }
                                    aria-current=move || if s.page.get() == page { "page" } else { "false" }
                                    on:click=move |_| s.page.set(page)
                                >
                                    <Icon name=nav_icon(page)/>
                                    <span class="nav-label">{page.label()}</span>
                                </button>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
            <div class="side-foot">
                <span
                    class=move || if s.live.get() { "live on" } else { "live" }
                    title=move || if s.live.get() { "Live-Verbindung aktiv" } else { "Keine Live-Verbindung" }
                >
                    <span class="live-dot"></span>
                    <span class="live-text">{move || if s.live.get() { "Live" } else { "Getrennt" }}</span>
                </span>
                <button
                    class="icon-btn"
                    aria-label="Farbschema wechseln"
                    title="Hell oder dunkel"
                    on:click=move |_| dark.set(jsutil::toggle_theme())
                >
                    {move || if dark.get() { view! { <Icon name="sun"/> } } else { view! { <Icon name="moon"/> } }}
                </button>
            </div>
        </nav>
    }
}

#[component]
fn Banner() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.error
                .get()
                .map(|message| {
                    view! {
                        <div class="banner" role="alert">
                            <Icon name="triangle-alert"/>
                            <span>
                                <strong>"Verbindungsproblem. "</strong>
                                {message}
                                " Es wird der letzte bekannte Stand angezeigt."
                            </span>
                        </div>
                    }
                })
        }}
    }
}

#[component]
fn Toast() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || s.toast.get().map(|text| view! { <div class="toast" role="status">{text}</div> })}
    }
}

#[component]
pub fn App() -> impl IntoView {
    let store = Store::new();
    provide_context(store);
    start(store);

    view! {
        <div class="app">
            <Sidebar/>
            <main class="main">
                <Banner/>
                {move || match store.page.get() {
                    Page::Overview => view! { <Overview/> }.into_any(),
                    Page::Map => view! { <MapView/> }.into_any(),
                    Page::Maintenance => view! { <Maintenance/> }.into_any(),
                }}
                <footer class="foot">
                    "Kartendaten von "
                    <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">"OpenStreetMap"</a>
                    ". Icons von "
                    <a href="https://lucide.dev" target="_blank" rel="noopener">"Lucide"</a>
                    "."
                </footer>
            </main>
            <Drawer/>
            <Toast/>
        </div>
    }
}
