//! Rahmen der Anwendung: Seitenleiste (auf dem Handy untere Leiste), Hinweise, Seitenwahl.

use leptos::prelude::*;

use crate::drawer::Drawer;
use crate::jsutil;
use crate::maintenance::Maintenance;
use crate::map_view::MapView;
use crate::overview::Overview;
use crate::state::{Page, Store, start};

/// Kleiner Waschbär als Markenzeichen (eigene Zeichnung).
#[component]
pub fn Logo() -> impl IntoView {
    view! {
        <svg class="logo" viewBox="0 0 64 64" aria-hidden="true">
            <circle cx="32" cy="32" r="30" fill="var(--accent)"></circle>
            <path d="M10 22 L18 8 L28 18 Z M54 22 L46 8 L36 18 Z" fill="#3a3238"></path>
            <ellipse cx="32" cy="36" rx="22" ry="19" fill="#9a928b"></ellipse>
            <path d="M10 33 Q32 22 54 33 L52 44 Q32 38 12 44 Z" fill="#3a3238"></path>
            <ellipse cx="22" cy="35" rx="5" ry="4" fill="#f3f0eb"></ellipse>
            <ellipse cx="42" cy="35" rx="5" ry="4" fill="#f3f0eb"></ellipse>
            <circle cx="22" cy="35" r="2" fill="#26232a"></circle>
            <circle cx="42" cy="35" r="2" fill="#26232a"></circle>
            <ellipse cx="32" cy="46" rx="4" ry="3" fill="#26232a"></ellipse>
        </svg>
    }
}

fn nav_icon(page: Page) -> impl IntoView {
    match page {
        Page::Overview => view! {
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"></path></svg>
        }
        .into_any(),
        Page::Map => view! {
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7zm0 9.5A2.5 2.5 0 1 1 12 6.500a2.500 2.500 0 0 1 0 5z"></path></svg>
        }
        .into_any(),
        Page::Maintenance => view! {
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14.700 6.300a4 4 0 0 0-5 5L3 18l3 3 6.700-6.700a4 4 0 0 0 5-5l-2.700 2.700-2.500-.5-.5-2.500z"></path></svg>
        }
        .into_any(),
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
                                    {nav_icon(page)}
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
                    class="theme-btn"
                    aria-label="Farbschema wechseln"
                    title="Hell oder dunkel"
                    on:click=move |_| dark.set(jsutil::toggle_theme())
                >
                    <svg viewBox="0 0 24 24" aria-hidden="true">
                        <path d="M12 3a9 9 0 1 0 9 9 7 7 0 0 1-9-9z"></path>
                    </svg>
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
                            <strong>"Verbindungsproblem. "</strong>
                            {message}
                            " Es wird der letzte bekannte Stand angezeigt."
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
                    "Messdaten der TrafficNodes. Kartendaten von "
                    <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">"OpenStreetMap"</a>
                    "."
                </footer>
            </main>
            <Drawer/>
            <Toast/>
        </div>
    }
}
