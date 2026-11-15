//! Eintragen bei Nodes: Wer sich bei einem Node einträgt, bekommt dessen Eilmeldungen im Postfach (und auf dem Handy,
//! wenn die Weiterleitung eingerichtet ist). Hat ein Node niemanden eingetragen, bekommen alle die Meldung, zum
//! Beispiel wenn die Batterie leer ist.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::Device;

use crate::api;
use crate::icons::Icon;
use crate::limit::{MoreButton, STEP};
use crate::overview::SimBadge;
use crate::state::{ModalKind, Store};

/// Die Nodes für die Liste: erst die eingetragenen, dann die übrigen, jeweils nach Kennung. Die Suche prüft Kennung
/// und Name ohne Rücksicht auf Groß- und Kleinschreibung.
pub fn matching(devices: &[Device], subscribed: &BTreeSet<String>, query: &str) -> Vec<(String, Option<String>, bool)> {
    let needle = query.trim().to_lowercase();
    let mut list: Vec<(String, Option<String>, bool)> = devices
        .iter()
        .filter(|d| {
            needle.is_empty()
                || d.id.to_lowercase().contains(&needle)
                || d.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
        })
        .map(|d| (d.id.clone(), d.name.clone(), subscribed.contains(&d.id)))
        .collect();
    list.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    list
}

/// Satz über den Stand der Einträge.
pub fn summary_text(count: usize) -> String {
    match count {
        0 => "Du bist bei keinem Node eingetragen. Du bekommst die Eilmeldungen der Nodes, bei denen sich niemand eingetragen hat."
            .to_owned(),
        1 => "Du bist bei 1 Node eingetragen und bekommst dessen Eilmeldungen.".to_owned(),
        n => format!("Du bist bei {n} Nodes eingetragen und bekommst deren Eilmeldungen."),
    }
}

/// Die Angaben im Text der Rückfrage, die dick geschrieben werden: das, was man vor dem Eintragen wissen muss.
pub const SUBSCRIBE_STRONG: [&str; 4] = [
    "Eilmeldungen dieses Nodes",
    "Postfach",
    "nur noch die Eingetragenen",
    "jederzeit wieder austragen",
];

/// Zerlegt einen Text in Stücke, die Angaben aus `strong` sind dabei als `true` markiert (dick zu schreiben).
pub fn emphasize(text: &str, strong: &[&str]) -> Vec<(String, bool)> {
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = strong
            .iter()
            .filter_map(|phrase| rest.find(phrase).map(|at| (at, *phrase)))
            .min_by_key(|(at, _)| *at);
        match next {
            Some((at, phrase)) => {
                if at > 0 {
                    parts.push((rest[..at].to_owned(), false));
                }
                parts.push((phrase.to_owned(), true));
                rest = &rest[at + phrase.len()..];
            }
            None => {
                parts.push((rest.to_owned(), false));
                break;
            }
        }
    }
    parts
}

/// Erklärung in der Rückfrage vor dem Eintragen: Was bedeutet das, wer wird benachrichtigt?
pub fn subscribe_text() -> String {
    "Eintragen heißt: Du bekommst ab jetzt die Eilmeldungen dieses Nodes, zum Beispiel wenn die Batterie fast leer ist \
     oder der Node ausfällt. Sie erscheinen in deinem Postfach und, wenn du die Weiterleitung eingerichtet hast, auch \
     per E-Mail. Sobald sich jemand bei einem Node einträgt, bekommen nur noch die \
     Eingetragenen seine Meldungen, alle anderen nicht mehr. Du kannst dich jederzeit wieder austragen."
        .to_owned()
}

/// Tipp auf Eintragen oder Austragen: Austragen geschieht sofort, Eintragen erst nach der Rückfrage.
pub fn request_toggle(s: Store, id: &str) {
    if s.subscriptions.with_untracked(|set| set.contains(id)) {
        toggle(s, id);
        s.say("Ausgetragen. Du bekommst die Eilmeldungen dieses Nodes nicht mehr.");
    } else {
        s.modal.set(Some(ModalKind::Subscribe {
            device_id: id.to_owned(),
        }));
    }
}

/// Trägt ein oder aus. Die Oberfläche folgt sofort, ein Fehler stellt den alten Stand wieder her.
pub fn toggle(s: Store, id: &str) {
    let id = id.to_owned();
    let was = s.subscriptions.with_untracked(|set| set.contains(&id));
    s.subscriptions.update(|set| {
        if was {
            set.remove(&id);
        } else {
            set.insert(id.clone());
        }
    });
    spawn_local(async move {
        let result = if was {
            api::unsubscribe(&id).await
        } else {
            api::subscribe(&id).await
        };
        if let Err(e) = result {
            s.say(e);
            s.subscriptions.update(|set| {
                if was {
                    set.insert(id.clone());
                } else {
                    set.remove(&id);
                }
            });
        }
    });
}

/// Knopf am Gerät: Eilmeldungen dieses Nodes bekommen.
#[component]
pub fn SubscribeButton(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let class_id = id.clone();
    let pressed_id = id.clone();
    let text_id = id.clone();
    let is_on = move |wanted: &str| s.subscriptions.with(|set| set.contains(wanted));
    view! {
        <button
            class=move || if is_on(&class_id) { "btn is-subscribed" } else { "btn" }
            aria-pressed=move || is_on(&pressed_id).to_string()
            title="Eilmeldungen dieses Nodes bekommen (mit Erklärung und Rückfrage)"
            on:click=move |_| request_toggle(s, &id)
        >
            <Icon name="bell"/>
            {move || if is_on(&text_id) { "Eingetragen" } else { "Eintragen" }}
        </button>
    }
}

/// Liste der Nodes mit Suche zum Ein- und Austragen.
#[component]
pub fn NodeSubscriptions() -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let shown = RwSignal::new(STEP);
    // Eine neue Suche beginnt wieder mit den ersten Treffern.
    Effect::new(move |_| {
        query.track();
        shown.set(STEP);
    });
    let list = Memo::new(move |_| {
        s.devices
            .with(|d| s.subscriptions.with(|subs| matching(d, subs, &query.get())))
    });
    let total = Signal::derive(move || list.with(Vec::len));
    let count = Memo::new(move |_| s.subscriptions.with(BTreeSet::len));
    view! {
        <section class="surface node-subs">
            <div class="surface-head">
                <h2>"Meine Nodes"</h2>
                <span class="count-pill mono-num">{move || count.get()}</span>
            </div>
            <div class="surface-body">
                <p class="muted">
                    "Trag dich bei Nodes ein, deren Eilmeldungen du bekommen willst (zum Beispiel Batterie fast leer oder Ausfall). Du bekommst sie im Postfach und, wenn du die Weiterleitung eingerichtet hast, auch auf dem Handy. Hat ein Node niemanden eingetragen, bekommen alle die Meldung. Vor dem Eintragen fragt die Seite kurz nach."
                </p>
                <p class="subs-summary">{move || summary_text(count.get())}</p>
                <label class="search">
                    <Icon name="search"/>
                    <input
                        type="search"
                        placeholder="Node suchen"
                        aria-label="Node suchen"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                </label>
                <ul class="subs-list">
                    {move || {
                        let items = list.get();
                        if items.is_empty() {
                            return view! { <li class="empty">"Kein Node passt zur Suche."</li> }.into_any();
                        }
                        items
                            .into_iter()
                            .take(shown.get())
                            .map(|(id, name, on)| {
                                let click_id = id.clone();
                                view! {
                                    <li class="subs-row">
                                        <label class="check">
                                            <input
                                                type="checkbox"
                                                prop:checked=on
                                                on:click=move |ev| {
                                                    // Der Haken ändert sich erst nach der Rückfrage, nicht schon beim Tippen.
                                                    ev.prevent_default();
                                                    request_toggle(s, &click_id);
                                                }
                                            />
                                            <span class="subs-name">
                                                <strong class="mono-num">{id.clone()}</strong>
                                                {name.map(|n| view! { <span class="muted">{n}</span> })}
                                            </span>
                                        </label>
                                        <SimBadge id=id/>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
                <MoreButton total=total shown=shown/>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use shared::{DataSource, DeviceStatus};

    fn device(id: &str, name: Option<&str>) -> Device {
        Device {
            id: id.into(),
            name: name.map(str::to_owned),
            status: DeviceStatus::Normal,
            soc_percent: None,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    #[test]
    fn subscribed_nodes_come_first_then_the_rest_by_id() {
        let devices = [device("C", None), device("A", None), device("B", Some("Nord"))];
        let subs: BTreeSet<String> = ["C".to_owned()].into();
        let ids: Vec<String> = matching(&devices, &subs, "").into_iter().map(|(id, ..)| id).collect();
        assert_eq!(ids, ["C", "A", "B"]);
        assert!(matching(&devices, &subs, "")[0].2);
    }

    #[test]
    fn the_search_finds_by_id_or_name_in_any_case() {
        let devices = [
            device("TN0001", Some("Kreuzung Nord")),
            device("TN0002", None),
            device("SIM01", None),
        ];
        let none = BTreeSet::new();
        let ids = |q: &str| -> Vec<String> { matching(&devices, &none, q).into_iter().map(|(id, ..)| id).collect() };
        assert_eq!(ids("tn"), ["TN0001", "TN0002"]);
        assert_eq!(ids("  NORD "), ["TN0001"]);
        assert_eq!(ids("sim"), ["SIM01"]);
        assert!(ids("gibtesnicht").is_empty());
        assert_eq!(ids("").len(), 3);
    }

    #[test]
    fn the_confirmation_explains_what_signing_up_means() {
        let text = subscribe_text();
        for word in ["Eilmeldungen", "Postfach", "Weiterleitung", "nur noch die", "austragen"] {
            assert!(text.contains(word), "{word}");
        }
    }

    #[test]
    fn the_important_parts_of_the_confirmation_are_bold() {
        let parts = emphasize(&subscribe_text(), &SUBSCRIBE_STRONG);
        assert_eq!(
            parts.iter().map(|(t, _)| t.as_str()).collect::<String>(),
            subscribe_text()
        );
        let bold: Vec<&str> = parts.iter().filter(|(_, b)| *b).map(|(t, _)| t.as_str()).collect();
        assert_eq!(bold, SUBSCRIBE_STRONG);
        assert_eq!(emphasize("nichts", &SUBSCRIBE_STRONG), [("nichts".to_owned(), false)]);
    }

    #[test]
    fn the_summary_says_what_happens() {
        assert!(summary_text(0).contains("niemand eingetragen hat"));
        assert!(summary_text(1).contains("1 Node eingetragen"));
        assert!(summary_text(7).contains("7 Nodes eingetragen"));
    }
}
