//! Batteriekatalog: Wer Batterien bestellt, legt sie hier mit Preis an. Beim Batteriewechsel wählt man sie dann nur noch
//! in einer Liste aus, Name und Preis kommen aus dem Katalog.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{BatteryType, NewBatteryType};

use crate::icons::Icon;
use crate::pager::Pager;
use crate::state::Store;
use crate::swap::parse_price_cents;
use crate::{api, format};

/// So viele Batterien stehen auf einer Seite des Katalogs. Ab der sechsten gibt es eine zweite Seite.
const BATTERY_PAGE_SIZE: usize = 5;

/// Eingaben des Formulars als neue Batterie. Leere Notiz entfällt, der Preis darf fehlen.
pub fn build_type(name: &str, price: &str, note: &str) -> Result<NewBatteryType, String> {
    let price_cents = parse_price_cents(price).map_err(str::to_owned)?;
    let new = NewBatteryType {
        name: name.trim().to_owned(),
        price_cents,
        note: Some(note.trim().to_owned()).filter(|n| !n.is_empty()),
    };
    new.validate().map_err(str::to_owned)?;
    Ok(new)
}

/// Preis fürs Formular beim Ändern, zum Beispiel `89,50`. Ohne Preis bleibt das Feld leer.
pub fn price_field(price_cents: Option<i32>) -> String {
    price_cents
        .map(|c| format::number(f64::from(c) / 100.0, 2))
        .unwrap_or_default()
}

#[component]
pub fn BatteryCatalogue() -> impl IntoView {
    let s = expect_context::<Store>();
    s.load_battery_types();
    let name = RwSignal::new(String::new());
    let price = RwSignal::new(String::new());
    let note = RwSignal::new(String::new());
    // Welche Batterie gerade geändert wird, `None` heißt neue anlegen.
    let editing = RwSignal::new(None::<i64>);
    let removing = RwSignal::new(None::<i64>);
    let problem = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // Die Liste der Batterien im Katalog lässt sich zuklappen, das Formular zum Anlegen steht darüber und bleibt sichtbar.
    let list_open = RwSignal::new(true);
    let page = RwSignal::new(0usize);
    let total = Signal::derive(move || s.battery_types.with(Vec::len));

    let reset = move || {
        name.set(String::new());
        price.set(String::new());
        note.set(String::new());
        editing.set(None);
        problem.set(None);
    };
    let save = move |_| {
        if busy.get_untracked() {
            return;
        }
        let new = match build_type(&name.get_untracked(), &price.get_untracked(), &note.get_untracked()) {
            Ok(n) => n,
            Err(e) => return problem.set(Some(e)),
        };
        problem.set(None);
        busy.set(true);
        let target = editing.get_untracked();
        spawn_local(async move {
            let result = match target {
                Some(id) => api::update_battery_type(id, &new).await,
                None => api::create_battery_type(&new).await,
            };
            match result {
                Ok(saved) => {
                    s.say(if target.is_some() {
                        format!("{} gespeichert.", saved.name)
                    } else {
                        format!("{} ist jetzt im Katalog.", saved.name)
                    });
                    reset();
                    s.load_battery_types();
                }
                Err(e) => problem.set(Some(e)),
            }
            busy.set(false);
        });
    };
    let edit = move |t: BatteryType| {
        name.set(t.name);
        price.set(price_field(t.price_cents));
        note.set(t.note.unwrap_or_default());
        editing.set(Some(t.id));
        problem.set(None);
        removing.set(None);
        // Das Formular steht über der Liste: dorthin führen, damit man sieht, dass es um diese Batterie geht.
        crate::jsutil::scroll_to_id("battery-form");
        crate::jsutil::focus_element("battery-name");
    };
    let remove = move |id: i64| {
        spawn_local(async move {
            match api::archive_battery_type(id).await {
                Ok(_) => {
                    removing.set(None);
                    if editing.get_untracked() == Some(id) {
                        reset();
                    }
                    s.say("Aus dem Katalog genommen.");
                    s.load_battery_types();
                }
                Err(e) => s.say(e),
            }
        });
    };

    view! {
        <section class="surface">
            <div class="surface-head">
                <h2>"Batteriekatalog"</h2>
            </div>
            <div class="surface-body">
                <h3 id="battery-form">{move || if editing.get().is_some() { "Batterie ändern" } else { "Neue Batterie anlegen" }}</h3>
                <label class="field">
                    <span>"Name (zum Beispiel Varta Silver 12 V 80 Ah)"</span>
                    <input
                        id="battery-name"
                        type="text"
                        maxlength="60"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Preis in Euro"</span>
                    <input
                        type="text"
                        inputmode="decimal"
                        placeholder="89,50"
                        prop:value=move || price.get()
                        on:input=move |ev| price.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Notiz (optional, zum Beispiel Händler oder Bestellnummer)"</span>
                    <input
                        type="text"
                        maxlength="300"
                        prop:value=move || note.get()
                        on:input=move |ev| note.set(event_target_value(&ev))
                    />
                </label>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                <div class="btn-row">
                    <button class="btn btn-primary" disabled=move || busy.get() on:click=save>
                        {move || if editing.get().is_some() { "Speichern" } else { "Anlegen" }}
                    </button>
                    <Show when=move || editing.get().is_some()>
                        <button class="btn" on:click=move |_| reset()>"Abbrechen"</button>
                    </Show>
                </div>
                <button
                    id="battery-catalogue"
                    class="fold-head battery-fold"
                    aria-expanded=move || list_open.get().to_string()
                    aria-label=move || if list_open.get() { "Katalog zuklappen" } else { "Katalog aufklappen" }
                    on:click=move |_| list_open.update(|o| *o = !*o)
                >
                    <h3>"Im Katalog"</h3>
                    <span class="fold-end">
                        <span class="count-pill mono-num">{move || s.battery_types.with(Vec::len)}</span>
                        {move || view! { <Icon name=if list_open.get() { "chevron-up" } else { "chevron-down" }/> }}
                    </span>
                </button>
                <Show when=move || list_open.get()>
                    <ul class="battery-list">
                        {move || {
                            let list = s.battery_types.get();
                            if list.is_empty() {
                                return view! { <li class="empty">"Noch keine Batterie im Katalog."</li> }.into_any();
                            }
                            // Ab mehr als fünf Batterien wird geblättert, damit die Liste nicht endlos wird.
                            let (start, end, _) = crate::filter::page_bounds(list.len(), page.get(), BATTERY_PAGE_SIZE);
                            list.into_iter()
                                .skip(start)
                                .take(end - start)
                                .map(|t| {
                                    let id = t.id;
                                    let for_edit = t.clone();
                                    let price = t.price_cents.map_or_else(|| "kein Preis".to_owned(), shared::euro_text);
                                    view! {
                                        <li class=move || {
                                            if editing.get() == Some(id) { "battery-row is-editing" } else { "battery-row" }
                                        }>
                                            <div class="battery-main">
                                                <strong>{t.name.clone()}</strong>
                                                <span class="muted mono-num">{price}</span>
                                                {t.note.clone().map(|n| view! { <span class="muted">{n}</span> })}
                                            </div>
                                            <div class="battery-actions">
                                                <button class="btn btn-sm" on:click=move |_| edit(for_edit.clone())>
                                                    "Ändern"
                                                </button>
                                                {move || {
                                                    if removing.get() == Some(id) {
                                                        view! {
                                                            <button class="btn btn-sm btn-danger" on:click=move |_| remove(id)>
                                                                "Wirklich entfernen"
                                                            </button>
                                                            <button class="btn btn-sm" on:click=move |_| removing.set(None)>
                                                                "Abbrechen"
                                                            </button>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {
                                                            <button class="btn btn-sm btn-soft-danger" on:click=move |_| removing.set(Some(id))>
                                                                "Entfernen"
                                                            </button>
                                                        }
                                                            .into_any()
                                                    }
                                                }}
                                            </div>
                                        </li>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </ul>
                    <Pager total=total page=page size=BATTERY_PAGE_SIZE label="Seiten des Katalogs" anchor="battery-catalogue"/>
                </Show>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_becomes_a_catalogue_entry() {
        let t = build_type("  Varta Silver  ", "89,50", "  Conrad ").unwrap();
        assert_eq!(t.name, "Varta Silver");
        assert_eq!(t.price_cents, Some(8_950));
        assert_eq!(t.note.as_deref(), Some("Conrad"));
        let bare = build_type("Exide", "", "  ").unwrap();
        assert_eq!((bare.price_cents, bare.note), (None, None));
    }

    #[test]
    fn a_missing_name_or_a_wrong_price_is_explained() {
        assert!(build_type("  ", "89", "").unwrap_err().contains("Namen"));
        assert!(build_type("Varta", "abc", "").unwrap_err().contains("89,50"));
        assert!(build_type("Varta", "10000,01", "").is_err());
    }

    #[test]
    fn the_price_field_shows_euro_with_a_comma() {
        assert_eq!(price_field(Some(8_950)), "89,50");
        assert_eq!(price_field(Some(9_900)), "99,00");
        assert_eq!(price_field(None), "");
    }
}
