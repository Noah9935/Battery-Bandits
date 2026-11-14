//! Touren planen: Eine Tour bekommt einen Namen, dann trägt man die Nodes ein, bei denen man die Batterie wechseln will.
//! Daraus entsteht die schnellste Reihenfolge und ein Link, der die Tour in Google Maps öffnet.
//!
//! Die Touren liegen im Browser dieses Geräts (wie die Merkliste), nicht auf dem Server.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use shared::Device;

use crate::icons::Icon;
use crate::maintenance::swap_matches;
use crate::pins::{pinned_devices, pinned_stops, start_text};
use crate::routing::{is_exact, ordered_legs};
use crate::state::Store;
use crate::tour::{Leg, maps_url, total_km};
use crate::{format, jsutil};

/// Schlüssel im Browser für die gespeicherten Touren.
const TOURS_KEY: &str = "bb-tours";
/// Längster Name einer Tour.
pub const MAX_NAME_CHARS: usize = 60;
/// Wie viele Treffer das Suchfeld beim Eintragen höchstens zeigt.
const HITS: usize = 6;

/// Eine benannte Tour mit den Nodes, bei denen die Batterie gewechselt werden soll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedTour {
    pub id: u64,
    pub name: String,
    pub devices: Vec<String>,
}

/// Gespeicherte Touren aus dem Text im Browser. Kaputter oder fehlender Inhalt ergibt eine leere Liste.
pub fn parse_tours(text: &str) -> Vec<PlannedTour> {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn tours_json(tours: &[PlannedTour]) -> String {
    serde_json::to_string(tours).unwrap_or_else(|_| "[]".to_owned())
}

/// Name einer Tour ohne Leerraum an den Rändern. Er darf nicht leer, nicht zu lang und nicht doppelt sein.
pub fn clean_name(name: &str, existing: &[PlannedTour]) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Bitte der Tour einen Namen geben, zum Beispiel Dienstag Nord.");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("Der Name ist zu lang (höchstens 60 Zeichen).");
    }
    if existing.iter().any(|t| t.name.to_lowercase() == name.to_lowercase()) {
        return Err("Eine Tour mit diesem Namen gibt es schon.");
    }
    Ok(name.to_owned())
}

/// Trägt einen Node ein. Gibt `false` zurück, wenn er schon dabei war.
pub fn add_device(tour: &mut PlannedTour, id: &str) -> bool {
    if tour.devices.iter().any(|d| d == id) {
        return false;
    }
    tour.devices.push(id.to_owned());
    true
}

pub fn remove_device(tour: &mut PlannedTour, id: &str) {
    tour.devices.retain(|d| d != id);
}

/// Die Nodes der Tour, die es noch gibt (ausgemusterte fehlen).
pub fn tour_devices(devices: &[Device], tour: &PlannedTour) -> Vec<Device> {
    let wanted: std::collections::BTreeSet<String> = tour.devices.iter().cloned().collect();
    pinned_devices(devices, &wanted)
}

/// Satz über den Stand der Tour: wie viele Nodes, wie weit, und was ohne Position fehlt.
pub fn summary_text(total: usize, legs: &[Leg]) -> String {
    if total == 0 {
        return "Noch keine Nodes in dieser Tour.".to_owned();
    }
    let nodes = if total == 1 {
        "1 Node".to_owned()
    } else {
        format!("{total} Nodes")
    };
    let km = format::number(total_km(legs), 1);
    let missing = total.saturating_sub(legs.len());
    let tail = if missing > 0 {
        format!(", {missing} ohne Position nicht dabei")
    } else {
        String::new()
    };
    format!("{nodes}, zusammen etwa {km} km Luftlinie{tail}.")
}

/// Seite "Tour planen": die Liste der Touren und eine geöffnete Tour mit ihren Nodes.
#[component]
pub fn TourPlanner() -> impl IntoView {
    let tours = RwSignal::new(parse_tours(&jsutil::storage_get(TOURS_KEY).unwrap_or_default()));
    Effect::new(move |_| jsutil::storage_set(TOURS_KEY, &tours_json(&tours.get())));
    let open = RwSignal::new(None::<u64>);
    let name = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);

    let create = move |_| {
        let result = tours.with_untracked(|t| clean_name(&name.get_untracked(), t));
        match result {
            Ok(clean) => {
                let id = jsutil::now_ms() as u64;
                tours.update(|t| {
                    t.push(PlannedTour {
                        id,
                        name: clean,
                        devices: Vec::new(),
                    })
                });
                name.set(String::new());
                problem.set(None);
                open.set(Some(id));
            }
            Err(e) => problem.set(Some(e.to_owned())),
        }
    };

    view! {
        {move || match open.get().filter(|id| tours.with(|t| t.iter().any(|t| t.id == *id))) {
            Some(id) => view! { <OpenTour tours=tours id=id open=open/> }.into_any(),
            None => {
                view! {
                    <section class="surface">
                        <div class="surface-head">
                            <h2>"Tour planen"</h2>
                            <span class="muted mono-num">{move || tours.with(Vec::len)}</span>
                        </div>
                        <div class="surface-body">
                            <p class="muted">
                                "Gib der Tour einen Namen, trage dann die Nodes ein, bei denen du die Batterie wechseln willst, und starte die schnellste Route in Google Maps."
                            </p>
                            <div class="tour-new">
                                <input
                                    type="text"
                                    maxlength="60"
                                    placeholder="Name der Tour, zum Beispiel Dienstag Nord"
                                    aria-label="Name der neuen Tour"
                                    prop:value=move || name.get()
                                    on:input=move |ev| name.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" {
                                            create(());
                                        }
                                    }
                                />
                                <button class="btn btn-primary" on:click=move |_| create(())>
                                    "Tour anlegen"
                                </button>
                            </div>
                            {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                            <ul class="tour-saved">
                                {move || {
                                    let list = tours.get();
                                    if list.is_empty() {
                                        return view! { <li class="empty">"Noch keine Tour geplant."</li> }.into_any();
                                    }
                                    list.into_iter()
                                        .map(|t| {
                                            let id = t.id;
                                            let n = t.devices.len();
                                            view! {
                                                <li class="tour-saved-row">
                                                    <button class="tour-saved-open" on:click=move |_| open.set(Some(id))>
                                                        <strong>{t.name.clone()}</strong>
                                                        <span class="muted mono-num">
                                                            {if n == 1 { "1 Node".to_owned() } else { format!("{n} Nodes") }}
                                                        </span>
                                                    </button>
                                                    <button
                                                        class="btn btn-sm btn-ghost"
                                                        aria-label=format!("Tour {} löschen", t.name)
                                                        on:click=move |_| tours.update(|t| t.retain(|t| t.id != id))
                                                    >
                                                        "Löschen"
                                                    </button>
                                                </li>
                                            }
                                        })
                                        .collect_view()
                                        .into_any()
                                }}
                            </ul>
                        </div>
                    </section>
                }
                    .into_any()
            }
        }}
    }
}

/// Eine geöffnete Tour: Nodes suchen und eintragen, Reihenfolge und Google-Maps-Link.
#[component]
fn OpenTour(tours: RwSignal<Vec<PlannedTour>>, id: u64, open: RwSignal<Option<u64>>) -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let tour = Memo::new(move |_| tours.with(|t| t.iter().find(|t| t.id == id).cloned()));
    let devices_in = Memo::new(move |_| {
        let Some(tour) = tour.get() else { return Vec::new() };
        s.devices.with(|d| tour_devices(d, &tour))
    });
    let (legs, url) = {
        let legs = Memo::new(move |_| ordered_legs(s.start_point(), devices_in.with(|d| pinned_stops(d))));
        let url = Memo::new(move |_| {
            let start = s.start_point();
            legs.with(|l| maps_url(start, l))
        });
        (legs, url)
    };
    let hits = Memo::new(move |_| {
        let taken = tour.with(|t| t.as_ref().map(|t| t.devices.clone()).unwrap_or_default());
        s.devices.with(|d| {
            let free: Vec<Device> = d.iter().filter(|d| !taken.contains(&d.id)).cloned().collect();
            swap_matches(&free, &query.get(), HITS)
        })
    });
    let add = move |device: String| {
        tours.update(|list| {
            if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                add_device(t, &device);
            }
        });
    };
    let remove = move |device: String| {
        tours.update(|list| {
            if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                remove_device(t, &device);
            }
        });
    };
    let add_favorites = move |_| {
        let favorites: Vec<String> = s.favorites.with(|f| f.iter().cloned().collect());
        let known: Vec<String> = s.devices.with(|d| d.iter().map(|d| d.id.clone()).collect());
        let mut added = 0;
        tours.update(|list| {
            if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                for device in favorites.iter().filter(|f| known.contains(f)) {
                    if add_device(t, device) {
                        added += 1;
                    }
                }
            }
        });
        s.say(match added {
            0 => "Die Merkliste hat keine neuen Nodes für diese Tour.".to_owned(),
            1 => "1 Node aus der Merkliste eingetragen.".to_owned(),
            n => format!("{n} Nodes aus der Merkliste eingetragen."),
        });
    };

    view! {
        <section class="surface">
            <div class="surface-head">
                <h2>{move || tour.get().map(|t| t.name).unwrap_or_default()}</h2>
                <button class="btn btn-sm" on:click=move |_| open.set(None)>"Alle Touren"</button>
            </div>
            <div class="surface-body">
                {move || {
                    url.get()
                        .map(|href| {
                            view! {
                                <a class="btn btn-primary btn-lg pin-route" href=href target="_blank" rel="noopener">
                                    <Icon name="navigation"/>
                                    "Schnellste Route starten (Google Maps)"
                                </a>
                            }
                        })
                }}
                <p class="muted">
                    {move || summary_text(devices_in.with(Vec::len), &legs.get())}
                    " "
                    {move || start_text(s.position.get().is_some(), s.home.get().is_some())}
                    "."
                    {move || (!is_exact(legs.with(Vec::len))).then_some(" Die Reihenfolge ist eine gute Näherung.")}
                </p>
                <div class="btn-row">
                    <button class="btn btn-sm" disabled=move || s.locating.get() on:click=move |_| s.locate()>
                        <Icon name="locate"/>
                        {move || if s.locating.get() { "Suche Standort" } else { "Standort aktualisieren" }}
                    </button>
                    <button class="btn btn-sm" on:click=add_favorites>
                        <Icon name="bookmark"/>
                        "Merkliste übernehmen"
                    </button>
                </div>
                {move || s.position_note.get().map(|n| crate::pins::position_hint(n, s.home.with(Option::is_some)))}
                <label class="field">
                    <span>"Node eintragen: Nummer oder Name tippen"</span>
                    <input
                        type="search"
                        autocomplete="off"
                        placeholder="Zum Beispiel SIM03"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                </label>
                {move || {
                    let (list, more) = hits.get();
                    if query.with(|q| q.trim().is_empty()) {
                        return ().into_any();
                    }
                    if list.is_empty() {
                        return view! { <p class="muted">"Kein freier Node passt zu dieser Suche."</p> }.into_any();
                    }
                    view! {
                        <ul class="swap-hits">
                            {list
                                .into_iter()
                                .map(|(device, title)| {
                                    view! {
                                        <li>
                                            <button class="btn swap-hit" on:click=move |_| add(device.clone())>
                                                            {title}
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ul>
                        {(more > 0)
                            .then(|| view! { <p class="muted">{format!("und {more} weitere, tippe mehr von der Nummer")}</p> })}
                    }
                        .into_any()
                }}
                <ol class="tour-list">
                    {move || {
                        let legs = legs.get();
                        let listed: Vec<String> = legs.iter().map(|l| l.stop.device_id.clone()).collect();
                        // Nodes ohne Position stehen am Ende, damit man sie sieht und entfernen kann.
                        let without: Vec<Device> = devices_in
                            .get()
                            .into_iter()
                            .filter(|d| !listed.contains(&d.id))
                            .collect();
                        if legs.is_empty() && without.is_empty() {
                            return view! { <li class="empty">"Noch keine Nodes eingetragen."</li> }.into_any();
                        }
                        let rows = legs
                            .into_iter()
                            .map(|leg| {
                                let device = leg.stop.device_id.clone();
                                view! {
                                    <li class="tour-stop">
                                        <span class="mono-num">{leg.stop.title.clone()}</span>
                                        <span class="muted">{leg.stop.reason.clone()}</span>
                                        <span class="tour-km mono-num muted">
                                            {match leg.km {
                                                Some(km) => format!("{} km", format::number(km, 1)),
                                                None => "Start".to_owned(),
                                            }}
                                        </span>
                                        <button
                                            class="btn btn-sm btn-ghost"
                                            aria-label=format!("{} aus der Tour entfernen", leg.stop.title)
                                            on:click=move |_| remove(device.clone())
                                        >
                                            "Entfernen"
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view();
                        let missing = without
                            .into_iter()
                            .map(|d| {
                                let device = d.id.clone();
                                view! {
                                    <li class="tour-stop">
                                        <span class="mono-num">{format::device_title(&d.id, d.name.as_deref())}</span>
                                        <span class="muted">"ohne Position"</span>
                                        <span class="tour-km"></span>
                                        <button class="btn btn-sm btn-ghost" on:click=move |_| remove(device.clone())>
                                            "Entfernen"
                                        </button>
                                    </li>
                                }
                            })
                            .collect_view();
                        view! { {rows}{missing} }.into_any()
                    }}
                </ol>
                <div class="btn-row">
                    <button
                        class="btn btn-ghost"
                        on:click=move |_| {
                            tours.update(|t| t.retain(|t| t.id != id));
                            open.set(None);
                        }
                    >
                        "Tour löschen"
                    </button>
                </div>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tour(name: &str, devices: &[&str]) -> PlannedTour {
        PlannedTour {
            id: 1,
            name: name.into(),
            devices: devices.iter().map(|d| (*d).to_owned()).collect(),
        }
    }

    #[test]
    fn a_tour_needs_a_new_name_of_sensible_length() {
        let existing = [tour("Dienstag Nord", &[])];
        assert_eq!(clean_name("  Mittwoch Süd ", &existing), Ok("Mittwoch Süd".to_owned()));
        assert!(clean_name("   ", &existing).unwrap_err().contains("Namen"));
        assert!(clean_name("dienstag nord", &existing).unwrap_err().contains("schon"));
        assert!(clean_name(&"x".repeat(61), &existing).unwrap_err().contains("zu lang"));
        assert!(clean_name(&"x".repeat(60), &existing).is_ok());
    }

    #[test]
    fn nodes_are_added_once_and_removed_again() {
        let mut t = tour("A", &["SIM01"]);
        assert!(add_device(&mut t, "SIM02"));
        assert!(!add_device(&mut t, "SIM02"), "schon dabei");
        remove_device(&mut t, "SIM01");
        assert_eq!(t.devices, ["SIM02"]);
        remove_device(&mut t, "SIM99");
        assert_eq!(t.devices, ["SIM02"]);
    }

    #[test]
    fn tours_survive_the_round_trip_through_the_browser_storage() {
        let list = vec![tour("Dienstag Nord", &["SIM01", "SIM02"]), tour("Süd", &[])];
        assert_eq!(parse_tours(&tours_json(&list)), list);
        assert!(parse_tours("").is_empty());
        assert!(parse_tours("kein json").is_empty());
        assert!(parse_tours("{\"x\":1}").is_empty());
    }

    #[test]
    fn the_summary_names_count_distance_and_missing_positions() {
        assert_eq!(summary_text(0, &[]), "Noch keine Nodes in dieser Tour.");
        let leg = |km| Leg {
            stop: crate::tour::Stop {
                device_id: "A".into(),
                title: "A".into(),
                lat: 0.0,
                lon: 0.0,
                reason: "x".into(),
                urgent: false,
            },
            km,
        };
        let legs = [leg(None), leg(Some(2.5))];
        assert_eq!(summary_text(2, &legs), "2 Nodes, zusammen etwa 2,5 km Luftlinie.");
        assert_eq!(
            summary_text(3, &legs),
            "3 Nodes, zusammen etwa 2,5 km Luftlinie, 1 ohne Position nicht dabei."
        );
        assert_eq!(summary_text(1, &legs[..1]), "1 Node, zusammen etwa 0,0 km Luftlinie.");
    }
}
