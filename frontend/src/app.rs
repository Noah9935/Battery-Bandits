//! Rahmen der Anwendung: Seitenleiste (auf dem Handy untere Leiste), Hinweise, Seitenwahl, globale Tastenkürzel.

use leptos::ev;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

use crate::drawer::Drawer;
use crate::features::Feature;
use crate::icons::Icon;
use crate::jsutil;
use crate::login::{LoginScreen, UserChip};
use crate::maintenance::{Maintenance, Options};
use crate::map_view::MapView;
use crate::overlays::{CommandPalette, HelpDialog, Modals, next_theme};
use crate::overview::Overview;
use crate::search::{KeyAction, key_action};
use crate::share;
use crate::state::{Page, Store};

/// Marke: der Waschbär mit eckigen Konturen (Datei unter assets/logo, von Trunk nach dist kopiert).
#[component]
pub fn Logo() -> impl IntoView {
    view! { <img class="logo" src="battery-bandits-logo.svg" alt="" width="48" height="48"/> }
}

fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Overview => "layout-dashboard",
        Page::Map => "map-pin",
        Page::Maintenance => "wrench",
        Page::Options => "settings",
    }
}

/// Zähler am Navigationspunkt: offene Meldungen (rot) bei der Übersicht, geplante Wechsel (kupfer) bei der Wartung.
fn nav_badge(s: Store, page: Page) -> Option<impl IntoView> {
    let (count, class) = match page {
        Page::Overview => (s.summary.get().open_alerts, "badge"),
        Page::Maintenance => (
            s.tasks.with(|t| {
                t.iter()
                    .filter(|t| t.status == shared::MaintenanceStatus::Planned)
                    .count() as i64
            }),
            "badge amber",
        ),
        _ => (0, "badge"),
    };
    (count > 0).then(|| view! { <span class=class>{count}</span> })
}

fn theme_icon(mode: &str) -> &'static str {
    match mode {
        "light" => "sun",
        "dark" => "moon",
        _ => "monitor",
    }
}

fn theme_label(mode: &str) -> &'static str {
    match mode {
        "light" => "Farbschema: hell",
        "dark" => "Farbschema: dunkel",
        _ => "Farbschema: automatisch",
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <nav class="side" aria-label="Hauptnavigation">
            <div class="brand">
                <Logo/>
                <span class="brand-name">"Battery Bandits"</span>
            </div>
            <div class="stripes side-stripes"></div>
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
                                    {move || nav_badge(s, page)}
                                </button>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
            <UserChip/>
            <div class="side-foot">
                <span
                    class=move || if s.live.get() { "live on" } else { "live" }
                    title=move || if s.live.get() { "Live-Verbindung aktiv" } else { "Keine Live-Verbindung" }
                >
                    <span class="live-dot"></span>
                    <span class="live-text">{move || if s.live.get() { "Live" } else { "Getrennt" }}</span>
                </span>
                <span class="side-buttons">
                    {move || {
                        s.on(Feature::Palette)
                            .then(|| {
                                view! {
                                    <button
                                        class="icon-btn"
                                        aria-label="Befehlspalette öffnen"
                                        title="Befehlspalette (Strg+K)"
                                        on:click=move |_| s.palette_open.set(true)
                                    >
                                        <Icon name="command"/>
                                    </button>
                                }
                            })
                    }}
                    <button
                        class="icon-btn"
                        aria-label="Farbschema wechseln"
                        title=move || theme_label(&s.theme_mode.get())
                        on:click=move |_| s.set_theme(next_theme(&s.theme_mode.get_untracked()))
                    >
                        {move || view! { <Icon name=theme_icon(&s.theme_mode.get())/> }}
                    </button>
                </span>
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

/// Globale Tastenkürzel, Link-Fragment für Filter und Tab-Titel mit der Zahl offener Meldungen.
fn wire_globals(s: Store) {
    let handle = window_event_listener(ev::keydown, move |event| {
        let Some(action) = key_action(&event.key(), event.ctrl_key() || event.meta_key(), jsutil::is_typing()) else {
            return;
        };
        match action {
            KeyAction::Close => {
                if s.modal.get_untracked().is_some() {
                    s.modal.set(None);
                } else if s.palette_open.get_untracked() {
                    s.palette_open.set(false);
                } else if s.help_open.get_untracked() {
                    s.help_open.set(false);
                } else {
                    s.selected.set(None);
                }
            }
            KeyAction::Palette if s.on(Feature::Palette) => {
                event.prevent_default();
                s.palette_open.update(|open| *open = !*open);
            }
            KeyAction::FocusSearch if s.on(Feature::Shortcuts) => {
                event.prevent_default();
                s.page.set(Page::Overview);
                jsutil::focus_element("search-input");
            }
            KeyAction::Page(index) if s.on(Feature::Shortcuts) => {
                if let Some(page) = Page::ALL.get(index) {
                    s.page.set(*page);
                }
            }
            KeyAction::ToggleTheme if s.on(Feature::Shortcuts) => {
                jsutil::toggle_theme();
                s.theme_mode.set(jsutil::theme_mode());
            }
            KeyAction::Help if s.on(Feature::Shortcuts) => s.help_open.update(|open| *open = !*open),
            _ => {}
        }
    });
    on_cleanup(move || handle.remove());

    // Filter aus dem Link lesen (einmal beim Start) und danach im Link mitführen.
    if s.on(Feature::Deeplink) {
        let hash = jsutil::location_hash();
        if hash.len() > 1 {
            s.filter.set(share::filter_from_hash(&hash));
        }
    }
    Effect::new(move |_| {
        let filter = s.filter.get();
        if s.on(Feature::Deeplink) {
            jsutil::replace_hash(&share::filter_to_hash(&filter));
        }
    });

    // Tab-Titel
    Effect::new(move |_| {
        let open = s.summary.get().open_alerts;
        let title = if s.on(Feature::Notify) && open > 0 {
            format!("({open}) Battery Bandits")
        } else {
            "Battery Bandits".to_owned()
        };
        jsutil::set_title(&title);
    });

    // Darstellung aus den gespeicherten Einstellungen anwenden
    Effect::new(move |_| jsutil::set_contrast(s.contrast_high.get()));
    Effect::new(move |_| jsutil::set_density(s.compact.get()));
}

#[component]
pub fn App() -> impl IntoView {
    let store = Store::new();
    provide_context(store);
    store.check_session();
    wire_globals(store);

    // Ein Memo meldet nur den Wechsel zwischen angemeldet und abgemeldet, nicht jede Änderung der Person.
    let signed_in = Memo::new(move |_| store.me.with(Option::is_some));

    view! {
        {move || {
            if !store.auth_checked.get() {
                view! { <div class="login-page" aria-busy="true"></div> }.into_any()
            } else if !signed_in.get() {
                view! { <LoginScreen/> }.into_any()
            } else {
                view! {
                    <div class="app">
                        <Sidebar/>
                        <main class="main">
                            <Banner/>
                            {move || match store.page.get() {
                                Page::Overview => view! { <Overview/> }.into_any(),
                                Page::Map => view! { <MapView/> }.into_any(),
                                Page::Maintenance => view! { <Maintenance/> }.into_any(),
                                Page::Options => view! { <Options/> }.into_any(),
                            }}
                            <footer class="foot">
                                "Kartendaten von "
                                <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">
                                    "OpenStreetMap"
                                </a>
                                ". Icons von "
                                <a href="https://lucide.dev" target="_blank" rel="noopener">"Lucide"</a>
                                "."
                            </footer>
                        </main>
                        <Drawer/>
                        <Modals/>
                        <CommandPalette/>
                        <HelpDialog/>
                        <Toast/>
                    </div>
                }
                    .into_any()
            }
        }}
    }
}
