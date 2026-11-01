//! Die Oberfläche: Rahmen, Navigation und Routen.

mod api;
mod chart;
mod device;
mod incidents;
mod live;
mod overview;
mod settings;
mod ui;

use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    let live = live::connect();
    provide_context(live);

    view! {
        <Router>
            <header class="topbar">
                <A href="/" attr:class="brand">
                    <img src="/raccoon.svg" alt="" width="30" height="30"/>
                    <span>"Battery Bandits"</span>
                </A>
                <nav aria-label="Hauptnavigation">
                    <A href="/" exact=true>"Übersicht"</A>
                    <A href="/meldungen">"Meldungen"</A>
                    <A href="/optionen">"Optionen"</A>
                </nav>
                <span
                    class="live"
                    class:on=move || live.connected.get()
                    title="Neue Messwerte erscheinen automatisch"
                >
                    <span class="dot"></span>
                    {move || if live.connected.get() { "Live" } else { "Verbinde" }}
                </span>
            </header>
            <main>
                <Routes fallback=|| view! { <p class="state">"Diese Seite gibt es nicht."</p> }>
                    <Route path=path!("/") view=overview::OverviewPage/>
                    <Route path=path!("/geraete/:id") view=device::DevicePage/>
                    <Route path=path!("/meldungen") view=incidents::IncidentsPage/>
                    <Route path=path!("/optionen") view=settings::SettingsPage/>
                </Routes>
            </main>
            <footer>
                <span>"Battery Bandits, TINF25D-KI, DHBW Stuttgart"</span>
                <span>"Waschbär-Grafik: Twemoji, CC-BY 4.0"</span>
            </footer>
        </Router>
    }
}
