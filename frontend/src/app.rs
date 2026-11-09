//! Rahmen der Anwendung: feste App-Shell (Kopfleiste, auf dem Handy untere Leiste), Hinweise, Seitenwahl,
//! globale Tastenkürzel. Die Shell füllt genau den Bildschirm aus, nur der Inhalt darin scrollt.

use leptos::ev;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

use crate::device::{DevicePage, Drawer};
use crate::features::Feature;
use crate::icons::Icon;
use crate::inbox::Inbox;
use crate::jsutil;
use crate::login::{LoginScreen, UserChip};
use crate::maintenance::{Maintenance, Options};
use crate::map_view::MapView;
use crate::overlays::{CommandPalette, HelpDialog, Modals, next_theme};
use crate::overview::Overview;
use crate::route;
use crate::search::{KeyAction, key_action};
use crate::share;
use crate::state::{Page, Store};

/// Marke: der Waschbär mit eckigen Konturen (Datei unter assets/logo, von Trunk nach dist kopiert).
#[component]
pub fn Logo() -> impl IntoView {
    view! { <img class="logo" src="/battery-bandits-logo.svg" alt="" width="24" height="24"/> }
}

fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Overview => "layout-dashboard",
        Page::Map => "map-pin",
        Page::Maintenance => "wrench",
        Page::Inbox => "inbox",
        Page::Options => "settings",
    }
}

/// Zähler am Navigationspunkt: offene Meldungen bei der Übersicht, geplante Einsätze bei der Wartung,
/// ungelesene Einträge im Postfach.
fn nav_badge(s: Store, page: Page) -> Option<impl IntoView> {
    let (count, class) = match page {
        Page::Overview => (s.summary.get().open_alerts, "badge crit"),
        Page::Maintenance => (
            s.tasks.with(|t| {
                t.iter()
                    .filter(|t| t.status == shared::MaintenanceStatus::Planned)
                    .count() as i64
            }),
            "badge",
        ),
        Page::Inbox => (s.inbox.with(|i| crate::inbox::unread_count(i)) as i64, "badge accent"),
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
fn TopBar() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <header class="topbar">
            <div class="tb-brand">
                <Logo/>
                <span class="tb-name">"Battery Bandits"</span>
            </div>
            <nav class="tb-nav" aria-label="Hauptnavigation">
                {Page::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, page)| {
                        let active = move || s.device_page.with(Option::is_none) && s.page.get() == page;
                        view! {
                            <button
                                class=move || if active() { "tb-link is-active" } else { "tb-link" }
                                aria-current=move || if active() { "page" } else { "false" }
                                on:click=move |_| s.go_page(page)
                            >
                                <Icon name=nav_icon(page)/>
                                <span class="tb-label">{page.label()}</span>
                                {move || nav_badge(s, page)}
                                <kbd class="tb-kbd" aria-hidden="true">{index + 1}</kbd>
                            </button>
                        }
                    })
                    .collect_view()}
            </nav>
            <div class="tb-right">
                {move || {
                    s.on(Feature::Palette)
                        .then(|| {
                            view! {
                                <button
                                    class="tb-palette"
                                    aria-label="Befehlspalette öffnen"
                                    title="Befehlspalette (Strg+K)"
                                    on:click=move |_| s.palette_open.set(true)
                                >
                                    <Icon name="search"/>
                                    <span class="tb-palette-text">"Suchen"</span>
                                    <kbd>"Strg K"</kbd>
                                </button>
                            }
                        })
                }}
                <span
                    class=move || if s.live.get() { "live is-on" } else { "live" }
                    title=move || if s.live.get() { "Live-Verbindung aktiv" } else { "Keine Live-Verbindung" }
                >
                    <span class="live-dot"></span>
                    <span class="live-text">{move || if s.live.get() { "Live" } else { "Getrennt" }}</span>
                </span>
                <button
                    class="icon-btn"
                    aria-label="Farbschema wechseln"
                    title=move || theme_label(&s.theme_mode.get())
                    on:click=move |_| s.set_theme(next_theme(&s.theme_mode.get_untracked()))
                >
                    {move || view! { <Icon name=theme_icon(&s.theme_mode.get())/> }}
                </button>
                <UserChip/>
            </div>
        </header>
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

/// Globale Tastenkürzel, Link-Fragment für Filter, Zurück-Knopf des Browsers und Tab-Titel mit der Zahl offener Meldungen.
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
                s.go_page(Page::Overview);
                jsutil::focus_element("search-input");
            }
            KeyAction::Page(index) if s.on(Feature::Shortcuts) => {
                if let Some(page) = Page::ALL.get(index) {
                    s.go_page(*page);
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

    // Zurück und Vor im Browser: Die Adresse bestimmt, ob eine Geräteseite offen ist.
    let pop = window_event_listener(ev::popstate, move |_| {
        s.selected.set(None);
        s.device_page.set(route::device_from_path(&jsutil::location_path()));
    });
    on_cleanup(move || pop.remove());

    // Filter aus dem Link lesen (einmal beim Start) und danach im Link mitführen, solange keine Geräteseite offen ist.
    if s.on(Feature::Deeplink) {
        let hash = jsutil::location_hash();
        if hash.len() > 1 {
            s.filter.set(share::filter_from_hash(&hash));
        }
    }
    Effect::new(move |_| {
        let filter = s.filter.get();
        if s.on(Feature::Deeplink) && s.device_page.with(Option::is_none) {
            jsutil::replace_hash(&share::filter_to_hash(&filter));
        }
    });

    // Tab-Titel
    Effect::new(move |_| {
        let open = s.summary.get().open_alerts;
        let device = s.device_page.get();
        let base = match device {
            Some(id) => format!("{id} | Battery Bandits"),
            None => "Battery Bandits".to_owned(),
        };
        let title = if s.on(Feature::Notify) && open > 0 {
            format!("({open}) {base}")
        } else {
            base
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
                view! { <div class="auth-page" aria-busy="true"></div> }.into_any()
            } else if !signed_in.get() {
                view! { <LoginScreen/> }.into_any()
            } else {
                view! {
                    <div class="shell">
                        <TopBar/>
                        <div class="stage">
                            <Banner/>
                            {move || match store.device_page.get() {
                                Some(id) => view! { <DevicePage id=id/> }.into_any(),
                                None => {
                                    match store.page.get() {
                                        Page::Overview => view! { <Overview/> }.into_any(),
                                        Page::Map => view! { <MapView/> }.into_any(),
                                        Page::Maintenance => view! { <Maintenance/> }.into_any(),
                                        Page::Inbox => view! { <Inbox/> }.into_any(),
                                        Page::Options => view! { <Options/> }.into_any(),
                                    }
                                }
                            }}
                        </div>
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
