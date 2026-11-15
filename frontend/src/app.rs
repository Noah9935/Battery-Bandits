//! Rahmen der Anwendung. Drei Inseln (Marke, Navigation, Werkzeuge) liegen über dem Inhalt, auf dem Handy
//! wandert die Navigation als Leiste nach unten.
//!
//! Für das Handy gebaut: Jede einmal geöffnete Seite bleibt im Speicher und wird nur ein- und ausgeblendet, der
//! Wechsel kostet deshalb kein Neuzeichnen. Zwischen den Seiten wechselt man auch durch Wischen. Fehler
//! erscheinen als Hinweis, den man wegwischt. Danach steht statt "Live" ein rotes Warndreieck in der Kopfzeile.

use leptos::ev;
use leptos::leptos_dom::helpers::window_event_listener;
use leptos::prelude::*;

use crate::device::{DevicePage, Drawer};
use crate::efficiency_view::EfficiencyPage;
use crate::features::Feature;
use crate::gesture;
use crate::icons::Icon;
use crate::inbox::Inbox;
use crate::jsutil;
use crate::login::{LoginScreen, UserChip};
use crate::maintenance::{Maintenance, Options};
use crate::map_view::MapView;
use crate::overlays::{CommandPalette, HelpDialog, Modals};
use crate::overview::Overview;
use crate::profile::ProfilePage;
use crate::route;
use crate::search::{KeyAction, key_action};
use crate::share;
use crate::state::{Page, Store};
use crate::swipe::{Swipeable, touch_point};

/// Marke: der Waschbär mit eckigen Konturen (Datei unter assets/logo, von Trunk nach dist kopiert).
#[component]
pub fn Logo() -> impl IntoView {
    view! { <img class="logo" src="/battery-bandits-logo.svg" alt="" width="28" height="28"/> }
}

fn nav_icon(page: Page) -> &'static str {
    match page {
        Page::Overview => "layout-dashboard",
        Page::Map => "map-pin",
        Page::Maintenance => "wrench",
        Page::Inbox => "inbox",
        Page::Efficiency => "activity",
        Page::Options => "settings",
        Page::Profile => "user",
        Page::Accounts => "user",
    }
}

/// Kurzer Name für die Navigation (auf dem Handy ist wenig Platz).
pub fn nav_label(page: Page) -> &'static str {
    match page {
        Page::Overview => "Übersicht",
        Page::Map => "Karte",
        Page::Maintenance => "Wartung",
        Page::Inbox => "Postfach",
        Page::Efficiency => "Effizienz",
        Page::Options => "Optionen",
        Page::Profile => "Profil",
        Page::Accounts => "Konten",
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

/// Klasse einer Seite: sichtbar oder nicht, und von welcher Seite sie hereinkommt.
pub fn page_class(active: bool, direction: i8) -> &'static str {
    match (active, direction) {
        (false, _) => "page",
        (true, d) if d < 0 => "page is-active from-left",
        (true, d) if d > 0 => "page is-active from-right",
        (true, _) => "page is-active",
    }
}

/// Zustand der Verbindung in der Kopfzeile. Solange alles läuft, ein grüner Punkt mit "Live". Gibt es ein Problem,
/// ein rotes Warndreieck: Ein Tipp darauf zeigt die Einzelheiten kurz an.
#[component]
fn StatusLight() -> impl IntoView {
    let s = expect_context::<Store>();
    let has_problem = Memo::new(move |_| s.problem().is_some());
    view! {
        {move || {
            if has_problem.get() {
                view! {
                    <button
                        class="alarm"
                        aria-label="Es gibt ein Problem. Einzelheiten anzeigen"
                        title="Es gibt ein Problem. Tippen zeigt die Einzelheiten."
                        on:click=move |_| s.toggle_problem()
                    >
                        <Icon name="triangle-alert"/>
                    </button>
                }
                    .into_any()
            } else {
                view! {
                    <span
                        class=move || if s.live.get() { "live is-on" } else { "live" }
                        title=move || if s.live.get() { "Live-Verbindung aktiv" } else { "Verbindung wird aufgebaut" }
                    >
                        <span class="live-dot"></span>
                        <span class="live-text">{move || if s.live.get() { "Live" } else { "Verbinde" }}</span>
                    </span>
                }
                    .into_any()
            }
        }}
    }
}

#[component]
fn TopBar() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <header class="topbar">
            <button class="island island-brand" aria-label="Zur Übersicht" on:click=move |_| s.go_page(Page::Overview)>
                <Logo/>
                <span class="wordmark">"Battery Bandits"</span>
            </button>
            <nav
                class=move || {
                    if s.device_page.with(Option::is_some) || s.page.get().nav_index().is_none() {
                        "island tb-nav no-active"
                    } else {
                        "island tb-nav"
                    }
                }
                style=move || format!("--i: {}", s.page.get().nav_index().unwrap_or(0))
                aria-label="Hauptnavigation"
            >
                {Page::NAV
                    .into_iter()
                    .map(|page| {
                        let active = move || s.device_page.with(Option::is_none) && s.page.get() == page;
                        view! {
                            <button
                                class=move || if active() { "tb-link is-active" } else { "tb-link" }
                                aria-current=move || if active() { "page" } else { "false" }
                                title=page.label()
                                on:click=move |_| s.go_page(page)
                            >
                                <Icon name=nav_icon(page)/>
                                <span class="tb-label">{nav_label(page)}</span>
                                {move || nav_badge(s, page)}
                            </button>
                        }
                    })
                    .collect_view()}
            </nav>
            <div class="island island-tools">
                {move || {
                    s.on(Feature::Palette)
                        .then(|| {
                            view! {
                                <button
                                    class="tb-palette"
                                    aria-label="Befehlspalette öffnen"
                                    title="Suchen und Befehle (Strg+K)"
                                    on:click=move |_| s.palette_open.set(true)
                                >
                                    <Icon name="search"/>
                                    <span class="keys" aria-hidden="true">
                                        <kbd>"Strg"</kbd>
                                        <span class="keys-plus">"+"</span>
                                        <kbd>"K"</kbd>
                                    </span>
                                </button>
                            }
                        })
                }}
                <StatusLight/>
                <UserChip/>
            </div>
        </header>
    }
}

/// Hinweis bei einem Problem. Er liegt über dem Inhalt, schiebt nichts weg, lässt sich wegwischen und
/// verschwindet nach einigen Sekunden von selbst. Das Warndreieck in der Kopfzeile bleibt, solange das Problem besteht.
#[component]
fn Banner() -> impl IntoView {
    let s = expect_context::<Store>();
    let problem = Memo::new(move |_| s.problem());
    // Ein neues Problem (oder ein anderer Text) zeigt den Hinweis wieder. Verschwindet das Problem, ist der Hinweis
    // für das nächste frisch.
    Effect::new(move |_| {
        problem.track();
        s.problem_hidden.set(false);
    });
    view! {
        <Show when=move || problem.with(Option::is_some) && !s.problem_hidden.get()>
            <Swipeable class="banner" role="alert" on_dismiss=move |()| s.problem_hidden.set(true)>
                <Icon name="triangle-alert"/>
                <span class="banner-text">
                    <strong>"Verbindungsproblem. "</strong>
                    {move || problem.get()}
                    " Es wird der letzte bekannte Stand angezeigt."
                </span>
                <button class="icon-btn banner-close" aria-label="Hinweis schließen" on:click=move |_| s.problem_hidden.set(true)>
                    <Icon name="x"/>
                </button>
            </Swipeable>
        </Show>
    }
}

/// Schwebender Knopf, der die letzte Aktion zurücknimmt (Batteriewechsel, Erledigt, Ausmustern, Merkliste leeren).
/// Er bleibt eine halbe Minute da.
#[component]
fn UndoButton() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.undo
                .get()
                .map(|(label, _)| {
                    let aria = format!("Rückgängig: {label}");
                    view! {
                        <button
                            class="undo-fab"
                            aria-label=aria
                            title="Die letzte Aktion zurücknehmen"
                            on:click=move |_| s.undo_last()
                        >
                            <Icon name="undo"/>
                            <span class="undo-text">
                                <strong>"Rückgängig"</strong>
                                <span>{label}</span>
                            </span>
                        </button>
                    }
                })
        }}
    }
}

#[component]
fn Toast() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            s.toast
                .get()
                .map(|text| {
                    view! {
                        <Swipeable class="toast" role="status" on_dismiss=move |()| s.toast.set(None)>
                            {text}
                        </Swipeable>
                    }
                })
        }}
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
                if let Some(page) = Page::NAV.get(index) {
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
    Effect::new(move |_| jsutil::set_density(s.compact.get()));
}

/// Beginn einer Berührung auf der Bühne: wo, wann, und ob Wischen hier überhaupt die Seite wechseln darf.
#[derive(Debug, Clone, Copy)]
struct TouchStart {
    x: f64,
    y: f64,
    at_ms: f64,
}

/// Die Bühne mit allen Seiten. Besuchte Seiten bleiben stehen und werden nur ausgeblendet. Waagerechtes Wischen
/// wechselt zur Nachbarseite, außer auf der Karte (dort verschiebt Wischen die Karte), in Bereichen, die selbst
/// auf Wischen reagieren, und solange ein Dialog oder das Geräteblatt offen ist.
#[component]
fn Stage() -> impl IntoView {
    let s = expect_context::<Store>();
    let start = StoredValue::new(None::<TouchStart>);
    let on_main = Memo::new(move |_| s.device_page.with(Option::is_none));
    let touch_start = move |ev: web_sys::TouchEvent| {
        let blocked = ev.touches().length() != 1
            || s.page.get_untracked() == Page::Map
            || s.device_page.with_untracked(Option::is_some)
            || s.selected.with_untracked(Option::is_some)
            || s.modal.with_untracked(Option::is_some)
            || s.palette_open.get_untracked()
            || s.help_open.get_untracked()
            || jsutil::in_swipe_free_zone(ev.target());
        start.set_value(match (blocked, touch_point(&ev, false)) {
            (false, Some((x, y))) => Some(TouchStart {
                x,
                y,
                at_ms: jsutil::now_ms(),
            }),
            _ => None,
        });
    };
    let touch_end = move |ev: web_sys::TouchEvent| {
        let (Some(from), Some((x, y))) = (start.get_value(), touch_point(&ev, true)) else {
            return;
        };
        start.set_value(None);
        let swipe = gesture::page_swipe(
            x - from.x,
            y - from.y,
            jsutil::now_ms() - from.at_ms,
            jsutil::viewport_width(),
        );
        if let Some(swipe) = swipe
            && let Some(current) = s.page.get_untracked().nav_index()
            && let Some(index) = gesture::neighbor(Page::NAV.len(), current, swipe)
        {
            s.go_page(Page::NAV[index]);
        }
    };
    view! {
        <div
            class="stage"
            on:touchstart=touch_start
            // Ein zweiter Finger (Zoomen) bricht das Wischen ab.
            on:touchmove=move |ev: web_sys::TouchEvent| {
                if ev.touches().length() != 1 {
                    start.set_value(None);
                }
            }
            on:touchend=touch_end
            on:touchcancel=move |_| start.set_value(None)
        >
            {Page::ALL
                .into_iter()
                .map(|page| {
                    let active = Memo::new(move |_| on_main.get() && s.page.get() == page);
                    let mounted = Memo::new(move |_| s.visited.with(|v| v.contains(&page)));
                    view! {
                        <section
                            class=move || page_class(active.get(), s.nav_dir.get())
                            hidden=move || !active.get()
                            aria-label=page.label()
                        >
                            <Show when=move || mounted.get()>
                                {match page {
                                    Page::Overview => view! { <Overview/> }.into_any(),
                                    Page::Map => view! { <MapView/> }.into_any(),
                                    Page::Maintenance => view! { <Maintenance/> }.into_any(),
                                    Page::Inbox => view! { <Inbox/> }.into_any(),
                                    Page::Efficiency => view! { <EfficiencyPage/> }.into_any(),
                                    Page::Options => view! { <Options/> }.into_any(),
                                    Page::Profile => view! { <ProfilePage/> }.into_any(),
                                    Page::Accounts => view! { <crate::admin::AccountsPage/> }.into_any(),
                                }}
                            </Show>
                        </section>
                    }
                })
                .collect_view()}
            {move || s.device_page.get().map(|id| view! { <section class="page is-active"><DevicePage id=id/></section> })}
        </div>
    }
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
                        <Stage/>
                        <Banner/>
                        <Drawer/>
                        <Modals/>
                        <CommandPalette/>
                        <HelpDialog/>
                        <Toast/>
                        <UndoButton/>
                    </div>
                }
                    .into_any()
            }
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_a_short_label_and_an_icon() {
        for page in Page::ALL {
            assert!(!nav_label(page).is_empty());
            assert!(
                nav_label(page).chars().count() <= 9,
                "{page:?}: passt sonst nicht in die untere Leiste"
            );
            assert!(crate::icons::svg(nav_icon(page)).is_some());
        }
    }

    #[test]
    fn the_navigation_has_five_pages_and_options_and_profile_live_behind_the_profile() {
        assert_eq!(Page::NAV.len(), 5);
        for (index, page) in Page::NAV.into_iter().enumerate() {
            assert_eq!(page.nav_index(), Some(index));
        }
        assert_eq!(Page::Options.nav_index(), None);
        assert_eq!(Page::Profile.nav_index(), None);
        assert_eq!(Page::ALL.len(), 8);
        for (index, page) in Page::ALL.into_iter().enumerate() {
            assert_eq!(page.index(), index);
        }
    }

    #[test]
    fn only_the_active_page_is_marked_and_slides_in_from_the_side_it_came() {
        assert_eq!(page_class(false, 1), "page");
        assert_eq!(page_class(true, 0), "page is-active");
        assert_eq!(page_class(true, -1), "page is-active from-left");
        assert_eq!(page_class(true, 1), "page is-active from-right");
    }
}
