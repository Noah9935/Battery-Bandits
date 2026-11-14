//! Wartung und Optionen: geplante Batteriewechsel und Schwellwerte.

use crate::features::Feature;
use crate::history::History;
use crate::icons::Icon;
use crate::inputs::{apply_anomaly_inputs, apply_inputs, number_text};
use crate::pins::PinList;
use crate::routing::{is_short_map_link, parse_map_location};
use crate::state::Store;
use crate::swap::SwapButton;
use crate::tickets::Tickets;
use crate::tour::TourView;
use crate::{api, jsutil};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
fn Thresholds() -> impl IntoView {
    let s = expect_context::<Store>();
    let critical = RwSignal::new(String::new());
    let warning = RwSignal::new(String::new());
    let minutes = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let filled = StoredValue::new(false);

    // Felder einmal mit den gespeicherten Werten füllen, sobald sie geladen sind.
    Effect::new(move |_| {
        if let Some(st) = s.settings.get()
            && !filled.get_value()
        {
            filled.set_value(true);
            critical.set(number_text(st.threshold_critical_pct));
            warning.set(number_text(st.threshold_warning_pct));
            minutes.set(number_text(st.offline_timeout_seconds as f32 / 60.0));
        }
    });

    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(current) = s.settings.get_untracked() else {
            return;
        };
        let new = match apply_inputs(&current, &critical.get(), &warning.get(), &minutes.get()) {
            Ok(n) => n,
            Err(e) => return problem.set(Some(e)),
        };
        spawn_local(async move {
            match api::save_settings(&new).await {
                Ok(saved) => {
                    s.settings.set(Some(saved));
                    problem.set(None);
                    s.say("Schwellwerte gespeichert. Die Geräte werden innerhalb weniger Sekunden neu bewertet.");
                }
                Err(e) => problem.set(Some(e)),
            }
        });
    };

    view! {
        <section class="surface surface-pad">
            <h2>"Schwellwerte"</h2>
            <p class="muted">"Gelten sofort für alle Geräte, ohne Neustart."</p>
            <form class="form" on:submit=save>
                <label class="field">
                    <span>"Kritisch unter (Prozent)"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        prop:value=move || critical.get()
                        on:input=move |ev| critical.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Warnung unter (Prozent)"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        prop:value=move || warning.get()
                        on:input=move |ev| warning.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Offline nach (Minuten ohne Messwert)"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        prop:value=move || minutes.get()
                        on:input=move |ev| minutes.set(event_target_value(&ev))
                    />
                </label>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                <button class="btn btn-primary" type="submit" disabled=move || s.settings.get().is_none()>
                    "Speichern"
                </button>
            </form>
        </section>
    }
}

#[component]
fn Anomalies() -> impl IntoView {
    let s = expect_context::<Store>();
    let enabled = RwSignal::new(true);
    let hours = RwSignal::new(String::new());
    let mv = RwSignal::new(String::new());
    let minutes = RwSignal::new(String::new());
    let fault = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let filled = StoredValue::new(false);

    Effect::new(move |_| {
        if let Some(st) = s.anomaly_settings.get()
            && !filled.get_value()
        {
            filled.set_value(true);
            enabled.set(st.enabled);
            hours.set(number_text(st.stagnation_hours));
            mv.set(st.drop_mv.to_string());
            minutes.set(st.drop_window_minutes.to_string());
            fault.set(number_text(st.fault_min_pct));
        }
    });

    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let new = match apply_anomaly_inputs(
            enabled.get_untracked(),
            &hours.get_untracked(),
            &mv.get_untracked(),
            &minutes.get_untracked(),
            &fault.get_untracked(),
        ) {
            Ok(n) => n,
            Err(e) => return problem.set(Some(e)),
        };
        spawn_local(async move {
            match api::save_anomaly_settings(&new).await {
                Ok(saved) => {
                    s.anomaly_settings.set(Some(saved));
                    problem.set(None);
                    s.say("Auffälligkeiten gespeichert.");
                }
                Err(e) => problem.set(Some(e)),
            }
        });
    };

    view! {
        <section class="surface surface-pad">
            <h2>"Auffälligkeiten"</h2>
            <p class="muted">
                "Neben leerer Batterie und Funkstille meldet das System, wenn Messwerte stehen bleiben, die Spannung einbricht oder ein gut geladenes Gerät ausfällt."
            </p>
            <form class="form" on:submit=save>
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || enabled.get()
                        on:change=move |ev| enabled.set(event_target_checked(&ev))
                    />
                    <span>"Auffälligkeiten erkennen"</span>
                </label>
                <label class="field">
                    <span>"Messwerte unverändert seit (Stunden)"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        prop:value=move || hours.get()
                        on:input=move |ev| hours.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Spannungseinbruch ab (Millivolt)"</span>
                    <input
                        type="text"
                        inputmode="numeric"
                        prop:value=move || mv.get()
                        on:input=move |ev| mv.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"... innerhalb von (Minuten)"</span>
                    <input
                        type="text"
                        inputmode="numeric"
                        prop:value=move || minutes.get()
                        on:input=move |ev| minutes.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Ausfall trotz Ladestand ab (Prozent)"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        prop:value=move || fault.get()
                        on:input=move |ev| fault.set(event_target_value(&ev))
                    />
                </label>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                <button class="btn btn-primary" type="submit" disabled=move || s.anomaly_settings.get().is_none()>
                    "Speichern"
                </button>
            </form>
        </section>
    }
}

/// Standardstandort: Start der Route, wenn der Browser keinen Standort liefert. Gewählt wird auf einer Karte (ein Tipp),
/// über einen Link aus Google Maps oder mit dem Standort des Telefons. Koordinaten tippt niemand von Hand.
#[component]
fn HomeLocation() -> impl IntoView {
    let s = expect_context::<Store>();
    let node = NodeRef::<leptos::html::Div>::new();
    let ready = RwSignal::new(false);
    let paste = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    node.on_load(move |_| {
        jsutil::next_frame(move || {
            let ok = jsutil::pick_init("bb-pick", s.home.get_untracked(), move |lat, lon| {
                s.set_home(Some((lat, lon)));
                problem.set(None);
                s.say("Standardstandort gespeichert.");
            });
            ready.set(ok);
        });
    });
    on_cleanup(jsutil::pick_destroy);
    // Die Seite bleibt im Speicher, auch unsichtbar: Beim Zurückkehren misst die Karte sich neu.
    Effect::new(move |_| {
        if s.page.get() == crate::state::Page::Options && s.device_page.with(Option::is_none) {
            jsutil::next_frame(jsutil::pick_invalidate);
        }
    });
    let choose = move |point: (f64, f64)| {
        s.set_home(Some(point));
        if ready.get_untracked() {
            jsutil::pick_set(point.0, point.1);
        }
        problem.set(None);
        s.say("Standardstandort gespeichert.");
    };
    let from_paste = move |_| {
        let text = paste.get_untracked();
        match parse_map_location(&text) {
            Some(point) => {
                choose(point);
                paste.set(String::new());
            }
            None if is_short_map_link(&text) => problem.set(Some(
                "Gekürzte Links (maps.app.goo.gl) lassen sich nicht lesen. In Google Maps den Ort lange drücken und den Link der Seite oder die Koordinaten oben kopieren.".to_owned(),
            )),
            None => problem.set(Some("Das ist weder ein Link aus Google Maps noch eine Koordinate.".to_owned())),
        }
    };
    let take_current = move |_| match s.position.get_untracked() {
        Some(point) => choose(point),
        None => s.locate(),
    };
    let clear = move |_| {
        s.set_home(None);
        if ready.get_untracked() {
            jsutil::pick_clear();
        }
        s.say("Standardstandort gelöscht.");
    };
    view! {
        <section class="surface surface-pad">
            <h2>"Standardstandort"</h2>
            <p class="muted">
                "Start der Route zu den gemerkten Geräten, wenn der Browser keinen Standort liefert. Tippe auf die Karte, um den Ort zu wählen. Gilt nur in diesem Browser."
            </p>
            <div id="bb-pick" class="pick-map" node_ref=node data-noswipe="" role="region" aria-label="Karte zur Auswahl des Standardstandorts"></div>
            <p class="muted pick-state">
                {move || if s.home.get().is_some() { "Ein Standort ist gespeichert (Markierung auf der Karte)." } else { "Noch kein Standort gewählt." }}
            </p>
            <div class="btn-row">
                <button class="btn" disabled=move || s.locating.get() on:click=take_current>
                    <Icon name="locate"/>
                    {move || {
                        if s.locating.get() {
                            "Suche Standort"
                        } else if s.position.get().is_some() {
                            "Meinen Standort nehmen"
                        } else {
                            "Meinen Standort abfragen"
                        }
                    }}
                </button>
                <a class="btn" href="https://www.google.com/maps" target="_blank" rel="noopener">
                    <Icon name="map-pin"/>
                    "In Google Maps wählen"
                </a>
                <button class="btn btn-ghost" disabled=move || s.home.get().is_none() on:click=clear>"Löschen"</button>
            </div>
            <div class="form">
                <label class="field">
                    <span>"Link aus Google Maps einfügen"</span>
                    <input
                        type="text"
                        placeholder="https://www.google.com/maps/@48.77,9.18,15z"
                        prop:value=move || paste.get()
                        on:input=move |ev| paste.set(event_target_value(&ev))
                    />
                </label>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                {move || s.position_note.get().map(|n| view! { <p class="muted">{n}</p> })}
                <button class="btn" disabled=move || paste.with(|p| p.trim().is_empty()) on:click=from_paste>"Übernehmen"</button>
            </div>
        </section>
    }
}

#[component]
fn Appearance() -> impl IntoView {
    let s = expect_context::<Store>();
    let permission = RwSignal::new(jsutil::notification_permission());
    view! {
        <section class="appearance-bar" aria-label="Darstellung">
            <h3>"Darstellung"</h3>
            <label class="field inline">
                <span>"Farbschema"</span>
                <select on:change=move |ev| s.set_theme(&event_target_value(&ev))>
                    <option value="auto" selected=move || s.theme_mode.get() == "auto">"Wie das System"</option>
                    <option value="light" selected=move || s.theme_mode.get() == "light">"Hell"</option>
                    <option value="dark" selected=move || s.theme_mode.get() == "dark">"Dunkel"</option>
                </select>
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || s.contrast_high.get()
                    on:change=move |ev| s.contrast_high.set(event_target_checked(&ev))
                />
                <span>"Hoher Kontrast"</span>
            </label>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || s.compact.get()
                    on:change=move |ev| s.compact.set(event_target_checked(&ev))
                />
                <span>"Kompakt"</span>
            </label>
            {move || {
                s.on(Feature::Notify)
                    .then(|| {
                        view! {
                            <button
                                class="btn btn-sm"
                                on:click=move |_| {
                                    jsutil::request_notification_permission();
                                    gloo_timers::callback::Timeout::new(1_500, move || {
                                        permission.set(jsutil::notification_permission());
                                    })
                                    .forget();
                                }
                            >
                                {move || match permission.get().as_str() {
                                    "granted" => "Browser-Hinweise erlaubt",
                                    "denied" => "Browser-Hinweise blockiert",
                                    _ => "Browser-Hinweise erlauben",
                                }}
                            </button>
                        }
                    })
            }}
        </section>
    }
}

#[component]
fn Lab() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="surface surface-pad">
            <h2>"Labor"</h2>
            <p class="muted">
                "Zusatzfunktionen einzeln ein- und ausschalten. Vorerst ist alles an, damit man in Ruhe durchgehen kann, was gefällt."
            </p>
            <div class="lab-actions">
                <button class="btn" on:click=move |_| s.set_all_features(true)>"Alle an"</button>
                <button class="btn" on:click=move |_| s.set_all_features(false)>"Alle aus"</button>
            </div>
            <ul class="lab-list">
                {Feature::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, feature)| {
                        view! {
                            <li class="lab-row">
                                <label>
                                    <input
                                        type="checkbox"
                                        prop:checked=move || s.on(feature)
                                        on:change=move |ev| s.set_feature(feature, event_target_checked(&ev))
                                    />
                                    <span class="lab-text">
                                        <strong>{format!("{}. {}", index + 1, feature.label())}</strong>
                                        <span class="muted">{feature.description()}</span>
                                    </span>
                                </label>
                            </li>
                        }
                    })
                    .collect_view()}
            </ul>
        </section>
    }
}

/// Wartung: geplante und erledigte Batteriewechsel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MaintenanceTab {
    Tickets,
    Pins,
    Tour,
    History,
}

impl MaintenanceTab {
    const ALL: [MaintenanceTab; 4] = [
        MaintenanceTab::Tickets,
        MaintenanceTab::Pins,
        MaintenanceTab::Tour,
        MaintenanceTab::History,
    ];

    fn label(self) -> &'static str {
        match self {
            MaintenanceTab::Tickets => "Einsätze",
            MaintenanceTab::Pins => "Merkliste",
            MaintenanceTab::Tour => "Tour",
            MaintenanceTab::History => "Protokoll",
        }
    }
}

/// Schnelleintrag: Gerät wählen, ein Tipp auf den Knopf, ein Tipp auf "Speichern". Das dringendste Gerät
/// ist vorgewählt.
#[component]
fn QuickSwap() -> impl IntoView {
    let s = expect_context::<Store>();
    let chosen = RwSignal::new(String::new());
    let target = Memo::new(move |_| {
        let picked = chosen.get();
        if !picked.is_empty() && s.devices.with(|d| d.iter().any(|d| d.id == picked)) {
            return picked;
        }
        s.devices.with(|d| {
            d.iter()
                .filter(|d| d.status.needs_attention())
                .min_by(|a, b| {
                    a.soc_percent
                        .unwrap_or(f32::MAX)
                        .total_cmp(&b.soc_percent.unwrap_or(f32::MAX))
                })
                .or_else(|| d.first())
                .map(|d| d.id.clone())
                .unwrap_or_default()
        })
    });
    view! {
        <section class="surface quick-swap">
            <div class="surface-head">
                <h2>"Batterie gewechselt?"</h2>
            </div>
            <div class="surface-body">
                <label class="field">
                    <span>"Gerät"</span>
                    <select on:change=move |ev| chosen.set(event_target_value(&ev))>
                        {move || {
                            let current = target.get();
                            s.devices
                                .get()
                                .into_iter()
                                .map(|d| {
                                    let selected = d.id == current;
                                    view! {
                                        <option value=d.id.clone() selected=selected>
                                            {crate::format::device_title(&d.id, d.name.as_deref())}
                                        </option>
                                    }
                                })
                                .collect_view()
                        }}
                    </select>
                </label>
                {move || {
                    let id = target.get();
                    (!id.is_empty()).then(|| view! { <SwapButton device_id=id class="btn-primary btn-lg"/> })
                }}
            </div>
        </section>
    }
}

#[component]
pub fn Maintenance() -> impl IntoView {
    let tab = RwSignal::new(MaintenanceTab::Tickets);
    view! {
        <div class="view">
        <header class="view-head">
            <h1>"Wartung"</h1>
            <p class="sub">"Einsätze planen, zuweisen und abschließen, Touren vorbereiten und nachlesen, wer was getan hat."</p>
        </header>
        <div class="tabs" role="tablist" aria-label="Bereich">
            {MaintenanceTab::ALL
                .into_iter()
                .map(|t| {
                    view! {
                        <button
                            class=move || if tab.get() == t { "seg-btn is-active" } else { "seg-btn" }
                            role="tab"
                            aria-selected=move || (tab.get() == t).to_string()
                            on:click=move |_| tab.set(t)
                        >
                            {t.label()}
                        </button>
                    }
                })
                .collect_view()}
        </div>
        {move || match tab.get() {
            MaintenanceTab::Tickets => view! { <QuickSwap/><Tickets/> }.into_any(),
            MaintenanceTab::Pins => view! { <PinList/><QuickSwap/> }.into_any(),
            MaintenanceTab::Tour => view! { <TourView/> }.into_any(),
            MaintenanceTab::History => {
                view! {
                    <section class="surface">
                        <div class="surface-head"><h2>"Protokoll"</h2></div>
                        <div class="surface-body">
                            <p class="muted">"Wer hat wann was getan: Einsätze, Zuweisungen, Kommentare, abgewiesene Meldungen."</p>
                            <History limit=100/>
                        </div>
                    </section>
                }
                    .into_any()
            }
        }}
        </div>
    }
}

/// Optionen: Schwellwerte, Darstellung und das Labor mit den schaltbaren Zusatzfunktionen.
#[component]
pub fn Options() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <div class="view">
            <header class="view-head">
                <h1>"Optionen"</h1>
                <p class="sub">"Schwellwerte, Auffälligkeiten, Standardstandort und Zusatzfunktionen."</p>
            </header>
            <div class="settings-grid">
                <Thresholds/>
                <Anomalies/>
                <HomeLocation/>
            </div>
            {move || {
                s.me
                    .with(|me| me.as_ref().is_some_and(|me| me.admin))
                    .then(|| view! { <crate::admin::AdminUsers/> })
            }}
            <Lab/>
            <Appearance/>
        </div>
    }
}
