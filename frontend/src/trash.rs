//! Papierkorb in den Optionen: Gelöschte Touren und aus dem Katalog genommene Batterien bleiben 30 Tage und lassen
//! sich zurückholen. Ausgemusterte Geräte stehen auch hier und lassen sich wieder aufnehmen.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{TRASH_DAYS, TrashedBattery, TrashedTour};

use crate::icons::Icon;
use crate::state::Store;
use crate::{api, jsutil};

/// Text für die verbleibende Zeit: `noch 12 Tage`, `noch 1 Tag`, `wird heute endgültig gelöscht`.
pub fn days_text(days: i64) -> String {
    match days {
        0 => "wird heute endgültig gelöscht".to_owned(),
        1 => "noch 1 Tag".to_owned(),
        n => format!("noch {n} Tage"),
    }
}

#[component]
pub fn Trash() -> impl IntoView {
    let s = expect_context::<Store>();
    let tours = RwSignal::new(Vec::<TrashedTour>::new());
    let load = move || {
        spawn_local(async move {
            if let Ok(list) = api::trashed_tours().await {
                tours.set(list);
            }
        });
    };
    load();
    // Lädt neu, wenn sich die Touren ändern (etwa nach dem Löschen in einem anderen Reiter).
    Effect::new(move |_| {
        s.tours.track();
        load();
    });
    let batteries = RwSignal::new(Vec::<TrashedBattery>::new());
    let load_batteries = move || {
        spawn_local(async move {
            if let Ok(list) = api::trashed_battery_types().await {
                batteries.set(list);
            }
        });
    };
    load_batteries();
    // Lädt neu, wenn sich der Katalog ändert (etwa nach dem Entfernen einer Batterie).
    Effect::new(move |_| {
        s.battery_types.track();
        load_batteries();
    });
    let restore_battery = move |id: i64| {
        spawn_local(async move {
            match api::restore_battery_type(id).await {
                Ok(_) => s.say("Die Batterie ist zurück im Katalog."),
                Err(e) => s.say(e),
            }
            s.load_battery_types();
            load_batteries();
        });
    };
    let purge_battery = move |id: i64| {
        spawn_local(async move {
            if let Err(e) = api::purge_battery_type(id).await {
                s.say(e);
            }
            load_batteries();
        });
    };
    let count = move || tours.with(Vec::len) + batteries.with(Vec::len) + s.retired.with(Vec::len);
    let restore = move |id: i64| {
        spawn_local(async move {
            match api::restore_tour(id).await {
                Ok(_) => s.say("Die Tour ist zurück in deiner Liste."),
                Err(e) => s.say(e),
            }
            s.load_tours();
            load();
        });
    };
    let purge = move |id: i64| {
        spawn_local(async move {
            if let Err(e) = api::purge_tour(id).await {
                s.say(e);
            }
            load();
        });
    };
    view! {
        <section id="opt-trash" class="surface surface-pad trash" aria-label="Papierkorb">
            <h2>
                <Icon name="trash"/>
                "Papierkorb"
                <span class="count-pill mono-num">{count}</span>
            </h2>
            {move || {
                (count() == 0).then(|| view! { <p class="muted trash-empty">"Der Papierkorb ist leer."</p> })
            }}
            <p class="muted">{format!("Wird automatisch nach {TRASH_DAYS} Tagen geleert.")}</p>
            <ul class="trash-items">
                {move || {
                    let now = chrono::DateTime::from_timestamp_millis(jsutil::now_ms() as i64).unwrap_or_default();
                    tours
                        .get()
                        .into_iter()
                        .map(|t| {
                            let id = t.id;
                            let days = t.days_left(now);
                            let nodes = if t.node_count == 1 { "1 Node".to_owned() } else { format!("{} Nodes", t.node_count) };
                            view! {
                                <li class="trash-item">
                                    <span class="trash-what">
                                        <strong>{t.name.clone()}</strong>
                                        <span class="muted">{format!("Tour, {nodes}, {}", days_text(days))}</span>
                                    </span>
                                    <button class="btn btn-sm" on:click=move |_| restore(id)>"Zurückholen"</button>
                                    <button class="btn btn-sm btn-ghost" on:click=move |_| purge(id)>"Endgültig löschen"</button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
                {move || {
                    let now = chrono::DateTime::from_timestamp_millis(jsutil::now_ms() as i64).unwrap_or_default();
                    batteries
                        .get()
                        .into_iter()
                        .map(|b| {
                            let id = b.id;
                            let days = b.days_left(now);
                            let price = b.price_cents.map_or_else(|| "kein Preis".to_owned(), shared::euro_text);
                            view! {
                                <li class="trash-item">
                                    <span class="trash-what">
                                        <strong>{b.name.clone()}</strong>
                                        <span class="muted">{format!("Batterie, {price}, {}", days_text(days))}</span>
                                    </span>
                                    <button class="btn btn-sm" on:click=move |_| restore_battery(id)>"Zurückholen"</button>
                                    <button class="btn btn-sm btn-ghost" on:click=move |_| purge_battery(id)>
                                        "Endgültig löschen"
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
                {move || {
                    s.retired
                        .get()
                        .into_iter()
                        .map(|d| {
                            let id = d.id.clone();
                            let back = d.id.clone();
                            let when = d.retired_at.map(jsutil::date_label).unwrap_or_default();
                            view! {
                                <li class="trash-item">
                                    <span class="trash-what">
                                        <strong class="mono-num">{id}</strong>
                                        <span class="muted">{format!("Ausgemustertes Gerät, seit {when}, bleibt bis zum Wiederaufnehmen")}</span>
                                    </span>
                                    <button class="btn btn-sm" on:click=move |_| crate::retire::reactivate(s, back.clone())>
                                        "Wieder aufnehmen"
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_remaining_time_reads_naturally() {
        assert_eq!(days_text(12), "noch 12 Tage");
        assert_eq!(days_text(1), "noch 1 Tag");
        assert_eq!(days_text(0), "wird heute endgültig gelöscht");
    }
}
