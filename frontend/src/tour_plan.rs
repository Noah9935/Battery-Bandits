//! Touren planen: Eine Tour bekommt einen Namen, dann trägt man die Nodes ein, bei denen man die Batterie wechseln will.
//! Daraus entsteht die schnellste Reihenfolge und ein Link, der die Tour in Google Maps öffnet.
//!
//! Die Touren liegen auf dem Server. Man kann eine eigene Tour anderen Konten freigeben: Wer sie bekommt, sieht sie
//! in seiner Liste "Mit mir geteilt", bekommt einen Hinweis im Postfach und kann die Route abgehen. Ändern, teilen
//! und löschen darf nur, wer sie angelegt hat.

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use shared::api::Me;
use shared::{Device, NewTour, ShareTour, Tour, TourUpdate};

use crate::icons::Icon;
use crate::maintenance::swap_matches;
use crate::people::PersonSearch;
use crate::pins::{pinned_devices, pinned_stops, start_text};
use crate::routing::{is_exact, ordered_legs};
use crate::state::Store;
use crate::swap::short_names;
use crate::tour::{Leg, maps_url, total_km};
use crate::{api, format};

/// Wie viele Treffer das Suchfeld beim Eintragen höchstens zeigt.
const HITS: usize = 6;

/// Eine Tour, wie sie früher im Browser lag (vor dem Umzug auf den Server). Nur noch zum einmaligen Übernehmen.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LegacyTour {
    pub name: String,
    pub devices: Vec<String>,
}

/// Alte Touren aus dem Text im Browser. Kaputter oder fehlender Inhalt ergibt eine leere Liste.
pub fn parse_legacy_tours(text: &str) -> Vec<LegacyTour> {
    serde_json::from_str(text).unwrap_or_default()
}

/// Name einer neuen Tour: nicht leer, nicht zu lang und unter den eigenen Touren noch nicht vergeben.
pub fn clean_name(name: &str, existing: &[Tour]) -> Result<String, &'static str> {
    let name = shared::tours::clean_tour_name(name)?;
    if existing
        .iter()
        .any(|t| t.mine && t.name.to_lowercase() == name.to_lowercase())
    {
        return Err("Eine Tour mit diesem Namen gibt es schon.");
    }
    Ok(name)
}

/// Die Nodes der Tour, die es noch gibt (ausgemusterte fehlen).
pub fn tour_devices(devices: &[Device], tour: &Tour) -> Vec<Device> {
    let wanted: BTreeSet<String> = tour.device_ids.iter().cloned().collect();
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

/// Anzeigenamen der Konten zu diesen Benutzernamen (unbekannte bleiben als Benutzername stehen).
pub fn names_of(usernames: &[String], users: &[Me]) -> Vec<String> {
    let short = short_names(users);
    usernames
        .iter()
        .map(|u| {
            short
                .iter()
                .find(|(name, _)| name == u)
                .map_or_else(|| u.clone(), |(_, label)| label.clone())
        })
        .collect()
}

/// Zeile unter dem Namen einer Tour in der Liste: von wem sie ist oder mit wem sie geteilt ist.
pub fn share_line(tour: &Tour, users: &[Me]) -> String {
    if !tour.mine {
        return format!("von {}", tour.owner_name);
    }
    if tour.shared_with.is_empty() {
        return "nicht geteilt".to_owned();
    }
    format!("geteilt mit {}", names_of(&tour.shared_with, users).join(", "))
}

/// Auswahl der Personen, mit denen geteilt wird: Suchfeld mit Menü und die Gewählten als Schilder zum Entfernen.
#[component]
pub fn ShareChooser(selected: RwSignal<BTreeSet<String>>) -> impl IntoView {
    let s = expect_context::<Store>();
    // Man selbst steht nicht im Menü, schon Gewählte auch nicht.
    let exclude = Signal::derive(move || {
        let mut set = selected.get();
        if let Some(me) = s.me.get() {
            set.insert(me.username);
        }
        set
    });
    view! {
        <PersonSearch
            on_pick=Callback::new(move |username: String| selected.update(|set| {
                set.insert(username);
            }))
            label=Signal::derive(String::new)
            exclude=exclude
            placeholder="Person suchen, um sie hinzuzufügen"
        />
        <div class="chips share-chosen" role="group" aria-label="Geteilt mit">
            {move || {
                let list: Vec<String> = selected.get().into_iter().collect();
                if list.is_empty() {
                    return view! { <span class="muted">"Noch niemand ausgewählt."</span> }.into_any();
                }
                let labels = s.users.with(|users| names_of(&list, users));
                list.into_iter()
                    .zip(labels)
                    .map(|(username, label)| {
                        let aria = format!("{label} wieder entfernen");
                        view! {
                            <button
                                class="chip is-active"
                                aria-label=aria
                                on:click=move |_| selected.update(|set| {
                                    set.remove(&username);
                                })
                            >
                                {label.clone()}
                                <Icon name="x"/>
                            </button>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

/// Seite "Tour planen": die Liste der Touren (eigene und geteilte) und eine geöffnete Tour mit ihren Nodes.
#[component]
pub fn TourPlanner() -> impl IntoView {
    let s = expect_context::<Store>();
    s.load_tours();
    let open = RwSignal::new(None::<i64>);
    let name = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);

    let create = move |_| {
        if busy.get_untracked() {
            return;
        }
        let clean = match s.tours.with_untracked(|t| clean_name(&name.get_untracked(), t)) {
            Ok(clean) => clean,
            Err(e) => return problem.set(Some(e.to_owned())),
        };
        problem.set(None);
        busy.set(true);
        spawn_local(async move {
            let new = NewTour {
                name: clean,
                ..NewTour::default()
            };
            match api::create_tour(&new).await {
                Ok(tour) => {
                    name.set(String::new());
                    open.set(Some(tour.id));
                    s.load_tours();
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };

    view! {
        {move || match open.get().filter(|id| s.tours.with(|t| t.iter().any(|t| t.id == *id))) {
            Some(id) => view! { <OpenTour id=id open=open/> }.into_any(),
            None => {
                view! {
                    <section class="surface">
                        <div class="surface-head">
                            <h2>"Tour planen"</h2>
                            <span class="muted mono-num">{move || s.tours.with(Vec::len)}</span>
                        </div>
                        <div class="surface-body">
                            <p class="muted">
                                "Gib der Tour einen Namen, trage dann die Nodes ein, bei denen du die Batterie wechseln willst, und starte die schnellste Route in Google Maps. Du kannst eine Tour mit anderen teilen, damit sie die Route abgehen."
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
                                <button class="btn btn-primary" disabled=move || busy.get() on:click=move |_| create(())>
                                    "Tour anlegen"
                                </button>
                            </div>
                            {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                            <ul class="tour-saved">
                                {move || {
                                    let list = s.tours.get();
                                    if list.is_empty() {
                                        return view! { <li class="empty">"Noch keine Tour geplant."</li> }.into_any();
                                    }
                                    let users = s.users.get();
                                    list.into_iter()
                                        .map(|t| {
                                            let id = t.id;
                                            let n = t.device_ids.len();
                                            let line = share_line(&t, &users);
                                            let mine = t.mine;
                                            let name_for_delete = t.name.clone();
                                            view! {
                                                <li class="tour-saved-row">
                                                    <button class="tour-saved-open" on:click=move |_| open.set(Some(id))>
                                                        <strong>{t.name.clone()}</strong>
                                                        <span class="muted mono-num">
                                                            {if n == 1 { "1 Node".to_owned() } else { format!("{n} Nodes") }}
                                                        </span>
                                                        <span class="muted tour-share-line">{line}</span>
                                                        <span class="tour-saved-go" aria-hidden="true">
                                                            "Öffnen"
                                                            <Icon name="chevron-right"/>
                                                        </span>
                                                    </button>
                                                    <button
                                                        class="btn btn-sm btn-ghost"
                                                        aria-label=format!("Tour {} {}", t.name, if mine { "löschen" } else { "aus meiner Liste nehmen" })
                                                        on:click=move |_| {
                                                            s.modal
                                                                .set(
                                                                    Some(crate::state::ModalKind::DeleteTour {
                                                                        id,
                                                                        name: name_for_delete.clone(),
                                                                        mine,
                                                                    }),
                                                                )
                                                        }
                                                    >
                                                        {if mine { "Löschen" } else { "Entfernen" }}
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

/// Setzt die Nodes einer eigenen Tour neu: erst in der Anzeige, dann auf dem Server.
fn save_devices(s: Store, id: i64, ids: Vec<String>) {
    s.tours.update(|list| {
        if let Some(t) = list.iter_mut().find(|t| t.id == id) {
            t.device_ids = ids.clone();
        }
    });
    spawn_local(async move {
        let update = TourUpdate {
            name: None,
            device_ids: Some(ids),
        };
        if let Err(e) = api::update_tour(id, &update).await {
            s.say(e);
        }
        s.load_tours();
    });
}

/// Eine geöffnete Tour: Nodes suchen und eintragen, Reihenfolge und Google-Maps-Link, teilen.
#[component]
fn OpenTour(id: i64, open: RwSignal<Option<i64>>) -> impl IntoView {
    let s = expect_context::<Store>();
    let query = RwSignal::new(String::new());
    let tour = Memo::new(move |_| s.tours.with(|t| t.iter().find(|t| t.id == id).cloned()));
    let mine = Memo::new(move |_| tour.with(|t| t.as_ref().is_some_and(|t| t.mine)));
    // Ist die Tour gelöscht (oder nicht mehr geteilt), geht die Ansicht zurück zur Liste.
    Effect::new(move |_| {
        if s.tours.with(|t| !t.iter().any(|t| t.id == id)) && !s.tours.with(Vec::is_empty) {
            open.set(None);
        }
    });
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
        let taken = tour.with(|t| t.as_ref().map(|t| t.device_ids.clone()).unwrap_or_default());
        s.devices.with(|d| {
            let free: Vec<Device> = d.iter().filter(|d| !taken.contains(&d.id)).cloned().collect();
            swap_matches(&free, &query.get(), HITS)
        })
    });
    let current_ids = move || tour.with_untracked(|t| t.as_ref().map(|t| t.device_ids.clone()).unwrap_or_default());
    let add = move |device: String| {
        let mut ids = current_ids();
        if !ids.contains(&device) {
            ids.push(device);
            save_devices(s, id, ids);
        }
    };
    let remove = move |device: String| {
        let mut ids = current_ids();
        ids.retain(|d| *d != device);
        save_devices(s, id, ids);
    };
    let add_favorites = move |_| {
        let favorites: Vec<String> = s.favorites.with(|f| f.iter().cloned().collect());
        let known: Vec<String> = s.devices.with(|d| d.iter().map(|d| d.id.clone()).collect());
        let mut ids = current_ids();
        let before = ids.len();
        for device in favorites.iter().filter(|f| known.contains(f)) {
            if !ids.contains(device) {
                ids.push(device.clone());
            }
        }
        let added = ids.len() - before;
        if added > 0 {
            save_devices(s, id, ids);
        }
        s.say(match added {
            0 => "Die Merkliste hat keine neuen Nodes für diese Tour.".to_owned(),
            1 => "1 Node aus der Merkliste eingetragen.".to_owned(),
            n => format!("{n} Nodes aus der Merkliste eingetragen."),
        });
    };
    // Eine geteilte Tour in die eigene Merkliste übernehmen.
    let take_over = move |_| {
        let ids = current_ids();
        spawn_local(async move {
            match api::add_pins(&ids).await {
                Ok(list) => {
                    s.favorites.set(list.into_iter().collect());
                    s.say("Die Nodes der Tour stehen jetzt in deiner Merkliste.");
                }
                Err(e) => s.say(e),
            }
        });
    };
    let leave = move |_| {
        spawn_local(async move {
            match api::leave_tour(id).await {
                Ok(_) => {
                    open.set(None);
                    s.say("Die Tour ist nicht mehr in deiner Liste.");
                }
                Err(e) => s.say(e),
            }
            s.load_tours();
        });
    };
    // Teilen: Die Auswahl startet mit den bisherigen Personen.
    let shares = RwSignal::new(tour.with_untracked(|t| {
        t.as_ref()
            .map(|t| t.shared_with.iter().cloned().collect::<BTreeSet<String>>())
            .unwrap_or_default()
    }));
    let sharing = RwSignal::new(false);
    let save_shares = move |_| {
        if sharing.get_untracked() {
            return;
        }
        sharing.set(true);
        let usernames: Vec<String> = shares.get_untracked().into_iter().collect();
        spawn_local(async move {
            match api::share_tour(id, &ShareTour { usernames }).await {
                Ok(saved) => {
                    s.say(if saved.shared_with.is_empty() {
                        "Die Tour ist nicht mehr geteilt.".to_owned()
                    } else {
                        "Freigabe gespeichert. Die Personen bekommen einen Hinweis im Postfach.".to_owned()
                    });
                }
                Err(e) => s.say(e),
            }
            s.load_tours();
            sharing.set(false);
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
                    tour.get()
                        .filter(|t| !t.mine)
                        .map(|t| {
                            view! {
                                <p class="muted tour-from">
                                    {format!("Diese Tour hat {} mit dir geteilt. Du kannst die Route abgehen, ändern darf sie nur {}.", t.owner_name, t.owner_name)}
                                </p>
                            }
                        })
                }}
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
                    <Show
                        when=move || mine.get()
                        fallback=move || {
                            view! {
                                <button class="btn btn-sm" on:click=take_over>
                                    <Icon name="bookmark"/>
                                    "In meine Merkliste"
                                </button>
                                <button class="btn btn-sm btn-ghost" on:click=leave>"Aus meiner Liste nehmen"</button>
                            }
                        }
                    >
                        <button class="btn btn-sm" on:click=add_favorites>
                            <Icon name="bookmark"/>
                            "Merkliste übernehmen"
                        </button>
                    </Show>
                </div>
                {move || s.position_note.get().map(|n| crate::pins::position_hint(n, s.home.with(Option::is_some)))}
                <Show when=move || mine.get()>
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
                </Show>
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
                        let editable = mine.get();
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
                                        {editable
                                            .then(|| {
                                                view! {
                                                    <button
                                                        class="btn btn-sm btn-ghost"
                                                        aria-label=format!("{} aus der Tour entfernen", leg.stop.title)
                                                        on:click=move |_| remove(device.clone())
                                                    >
                                                        "Entfernen"
                                                    </button>
                                                }
                                            })}
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
                                        {editable
                                            .then(|| {
                                                view! {
                                                    <button class="btn btn-sm btn-ghost" on:click=move |_| remove(device.clone())>
                                                        "Entfernen"
                                                    </button>
                                                }
                                            })}
                                    </li>
                                }
                            })
                            .collect_view();
                        view! { {rows}{missing} }.into_any()
                    }}
                </ol>
                <Show when=move || mine.get()>
                    <h3>"Mit anderen teilen"</h3>
                    <p class="muted">
                        "Wer die Tour bekommt, sieht sie in seiner Liste und kann die Route abgehen. Ändern kann nur, wer sie angelegt hat."
                    </p>
                    <ShareChooser selected=shares/>
                    <div class="btn-row">
                        <button class="btn btn-primary" disabled=move || sharing.get() on:click=save_shares>
                            "Freigabe speichern"
                        </button>
                    </div>
                    <div class="btn-row">
                        <button
                            class="btn btn-ghost"
                            on:click=move |_| {
                                let name = tour.with_untracked(|t| t.as_ref().map(|t| t.name.clone()).unwrap_or_default());
                                s.modal.set(Some(crate::state::ModalKind::DeleteTour { id, name, mine: true }));
                            }
                        >
                            "Tour löschen"
                        </button>
                    </div>
                </Show>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tour(name: &str, devices: &[&str], mine: bool) -> Tour {
        Tour {
            id: 1,
            name: name.into(),
            owner: if mine { "noah" } else { "fynn" }.into(),
            owner_name: if mine { "Noah Boufercha" } else { "Fynn Becker" }.into(),
            device_ids: devices.iter().map(|d| (*d).to_owned()).collect(),
            shared_with: Vec::new(),
            mine,
            updated_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    fn user(username: &str, name: &str) -> Me {
        Me {
            username: username.into(),
            display_name: name.into(),
            avatar: 0,
            admin: false,
        }
    }

    #[test]
    fn a_tour_needs_a_new_name_of_sensible_length() {
        let existing = [tour("Dienstag Nord", &[], true)];
        assert_eq!(clean_name("  Mittwoch Süd ", &existing), Ok("Mittwoch Süd".to_owned()));
        assert!(clean_name("   ", &existing).unwrap_err().contains("Namen"));
        assert!(clean_name("dienstag nord", &existing).unwrap_err().contains("schon"));
        assert!(clean_name(&"x".repeat(61), &existing).unwrap_err().contains("zu lang"));
        assert!(clean_name(&"x".repeat(60), &existing).is_ok());
    }

    #[test]
    fn a_name_used_only_by_someone_elses_tour_is_free() {
        let existing = [tour("Süd", &[], false)];
        assert!(clean_name("Süd", &existing).is_ok(), "der Name gilt je Person");
    }

    #[test]
    fn old_browser_tours_can_still_be_read() {
        let old = r#"[{"id":1,"name":"Dienstag Nord","devices":["SIM01","SIM02"]},{"id":2,"name":"Süd","devices":[]}]"#;
        let list = parse_legacy_tours(old);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].devices, ["SIM01", "SIM02"]);
        assert!(parse_legacy_tours("").is_empty());
        assert!(parse_legacy_tours("kein json").is_empty());
        assert!(parse_legacy_tours("{\"x\":1}").is_empty());
    }

    #[test]
    fn the_list_says_whose_tour_it_is_and_who_may_walk_it() {
        let users = [user("noah", "Noah Boufercha"), user("fynn", "Fynn Becker")];
        let mut own = tour("A", &[], true);
        assert_eq!(share_line(&own, &users), "nicht geteilt");
        own.shared_with = vec!["fynn".into()];
        assert_eq!(share_line(&own, &users), "geteilt mit Fynn B.");
        let received = tour("B", &[], false);
        assert_eq!(share_line(&received, &users), "von Fynn Becker");
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
