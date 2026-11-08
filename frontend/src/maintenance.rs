//! Wartung und Optionen: geplante Batteriewechsel und Schwellwerte.

use crate::features::Feature;
use crate::inputs::{apply_inputs, number_text};
use crate::state::Store;
use crate::tickets::Tickets;
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
        <section class="panel padded">
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
                <button class="btn primary" type="submit" disabled=move || s.settings.get().is_none()>
                    "Speichern"
                </button>
            </form>
        </section>
    }
}

#[component]
fn Appearance() -> impl IntoView {
    let s = expect_context::<Store>();
    let permission = RwSignal::new(jsutil::notification_permission());
    view! {
        <section class="panel padded">
            <h2>"Darstellung"</h2>
            <p class="muted">"Gilt nur in diesem Browser."</p>
            <div class="form">
                <label class="field">
                    <span>"Farbschema"</span>
                    <select on:change=move |ev| s.set_theme(&event_target_value(&ev))>
                        <option value="auto" selected=move || s.theme_mode.get() == "auto">"Automatisch (wie das System)"</option>
                        <option value="light" selected=move || s.theme_mode.get() == "light">"Hell, Waschbär bei Tag"</option>
                        <option value="dark" selected=move || s.theme_mode.get() == "dark">"Dunkel, Waschbär bei Nacht"</option>
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
                    <span>"Kompakte Tabelle"</span>
                </label>
                {move || {
                    s.on(Feature::Notify)
                        .then(|| {
                            view! {
                                <div class="notify-row">
                                    <button
                                        class="btn"
                                        on:click=move |_| {
                                            jsutil::request_notification_permission();
                                            gloo_timers::callback::Timeout::new(1_500, move || {
                                                permission.set(jsutil::notification_permission());
                                            })
                                            .forget();
                                        }
                                    >
                                        "Browser-Benachrichtigungen erlauben"
                                    </button>
                                    <span class="muted">
                                        {move || match permission.get().as_str() {
                                            "granted" => "Erlaubt.",
                                            "denied" => "Im Browser blockiert.",
                                            _ => "Noch nicht erlaubt.",
                                        }}
                                    </span>
                                </div>
                            }
                        })
                }}
            </div>
        </section>
    }
}

#[component]
fn Lab() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="panel padded">
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
#[component]
pub fn Maintenance() -> impl IntoView {
    view! {
        <header class="page-head">
            <h1>"Wartung"</h1>
            <p class="sub">"Einsätze planen, zuweisen, kommentieren und abschließen."</p>
        </header>
        <Tickets/>
    }
}

/// Optionen: Schwellwerte, Darstellung und das Labor mit den schaltbaren Zusatzfunktionen.
#[component]
pub fn Options() -> impl IntoView {
    view! {
        <header class="page-head">
            <h1>"Optionen"</h1>
            <p class="sub">"Schwellwerte, Darstellung und Zusatzfunktionen."</p>
        </header>
        <div class="two-col">
            <Thresholds/>
            <Appearance/>
        </div>
        <Lab/>
    }
}
