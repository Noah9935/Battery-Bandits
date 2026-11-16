//! Schnellerfassung des Batteriewechsels: ein Tipp auf den Knopf, ein Tipp auf "Speichern". Gerät und Zeitpunkt
//! sind schon eingetragen. Den Ladestand der neuen Batterie muss niemand kennen: Die Batterien werden geladen
//! mitgebracht, das System schätzt den Stand mit der ersten Messung aus der Spannung. Wer ihn weiß, kann ihn angeben.
//! Wer gewechselt hat, der Preis und eine Notiz sind optional.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::BatterySwap;
use shared::api::Me;

use std::collections::BTreeSet;

use crate::icons::Icon;
use crate::people::PersonSearch;
use crate::state::{ModalKind, Store};
use crate::{api, format};

/// Schlüssel im Browser für das zuletzt eingetragene Batteriemodell.
const LAST_MODEL_KEY: &str = "bb-last-model";
/// Schlüssel im Browser für die zuletzt gewählte Batterie aus dem Katalog.
const LAST_TYPE_KEY: &str = "bb-last-battery-type";
/// Auswahlwert für "Andere Batterie": Name und Preis werden frei eingetragen.
const OTHER: &str = "other";

/// Die Batterie aus dem Katalog, die zum Auswahlwert passt (`""`, `other` oder eine Kennung). Eine Kennung, die es
/// nicht mehr gibt, gilt als "keine Angabe".
pub fn chosen_type(choice: &str, types: &[shared::BatteryType]) -> Option<i64> {
    let id: i64 = choice.parse().ok()?;
    types.iter().any(|t| t.id == id).then_some(id)
}

/// Spannung einer Batterie mit diesem Ladestand (linear zwischen leer und voll, wie bei der Schätzung des Startwerts).
pub fn soc_voltage_mv(soc: f32, min_mv: i32, max_mv: i32) -> i32 {
    let soc = f64::from(soc.clamp(0.0, 100.0));
    (f64::from(min_mv) + (f64::from(max_mv) - f64::from(min_mv)) * soc / 100.0).round() as i32
}

/// Zeile unter der Auswahl: `Neuer Ladestand: 100 % (12,70 V)` oder der Hinweis auf die Schätzung.
pub fn charge_line(soc: Option<f32>, min_mv: i32, max_mv: i32) -> String {
    match soc {
        Some(soc) => format!(
            "Neuer Ladestand: {} ({})",
            format::percent(soc),
            format::voltage(soc_voltage_mv(soc, min_mv, max_mv))
        ),
        None => "Neuer Ladestand: wird mit der ersten Messung geschätzt".to_owned(),
    }
}

/// Ladestand aus der Eingabe in Prozent. Leer bedeutet "automatisch". Erlaubt sind `95`, `95,5`, `95.5` und `95 %`.
pub fn parse_soc(text: &str) -> Result<Option<f32>, &'static str> {
    const WRONG: &str = "Bitte einen Ladestand von 0 bis 100 Prozent eingeben, zum Beispiel 95.";
    let cleaned = text.trim().trim_end_matches('%').trim().replace(',', ".");
    if cleaned.is_empty() {
        return Ok(None);
    }
    let value: f32 = cleaned.parse().map_err(|_| WRONG)?;
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(WRONG);
    }
    Ok(Some((value * 10.0).round() / 10.0))
}

/// Preis aus der Eingabe in Cent. Leer ergibt keinen Preis. Erlaubt sind `89`, `89,5`, `89,50` und `89.50`,
/// dazu ein angehängtes `€` oder `EUR`.
pub fn parse_price_cents(text: &str) -> Result<Option<i32>, &'static str> {
    const WRONG: &str = "Bitte den Preis in Euro eingeben, zum Beispiel 89,50.";
    let cleaned = text
        .trim()
        .trim_end_matches("EUR")
        .trim_end_matches("eur")
        .trim_end_matches('€')
        .trim()
        .replace(',', ".");
    if cleaned.is_empty() {
        return Ok(None);
    }
    let (whole, fraction) = cleaned.split_once('.').unwrap_or((cleaned.as_str(), ""));
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || fraction.len() > 2 || whole.len() > 5 {
        return Err(WRONG);
    }
    let euros: i32 = whole.parse().map_err(|_| WRONG)?;
    let cents: i32 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i32>().map_err(|_| WRONG)? * 10,
        _ => fraction.parse().map_err(|_| WRONG)?,
    };
    let total = euros * 100 + cents;
    if total > BatterySwap::MAX_PRICE_CENTS {
        return Err("Der Preis darf höchstens 10.000 Euro betragen.");
    }
    Ok(Some(total))
}

/// Namen für die Auswahl: Vorname und Initiale des Nachnamens (`Noah B.`), solange das eindeutig ist. Teilen sich
/// zwei Personen dieselbe Kurzform, stehen beide mit vollem Namen da. Gibt es auch den doppelt, folgt die Kennung.
pub fn short_names(users: &[Me]) -> Vec<(String, String)> {
    let short = |name: &str| -> String {
        let parts: Vec<&str> = name.split_whitespace().collect();
        match (parts.first(), parts.last()) {
            (Some(first), Some(last)) if parts.len() >= 2 => {
                format!(
                    "{first} {}.",
                    last.chars()
                        .next()
                        .map(|c| c.to_uppercase().to_string())
                        .unwrap_or_default()
                )
            }
            _ => name.trim().to_owned(),
        }
    };
    let candidates: Vec<String> = users.iter().map(|u| short(&u.display_name)).collect();
    let count = |list: &[String], label: &str| list.iter().filter(|l| l.to_lowercase() == label.to_lowercase()).count();
    let full: Vec<String> = users.iter().map(|u| u.display_name.trim().to_owned()).collect();
    users
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let label = if count(&candidates, &candidates[i]) == 1 {
                candidates[i].clone()
            } else if count(&full, &full[i]) == 1 {
                full[i].clone()
            } else {
                format!("{} ({})", full[i], u.username)
            };
            (u.username.clone(), label)
        })
        .collect()
}

/// Personen, die zur Suche passen, und immer auch die gewählte, damit sie nicht aus dem Blick verschwindet.
/// Gesucht wird in Name und Kennung, ohne auf Groß- und Kleinschreibung zu achten.
pub fn visible_people(people: &[(String, String)], users: &[Me], query: &str, selected: &str) -> Vec<(String, String)> {
    let needle = query.trim().to_lowercase();
    people
        .iter()
        .filter(|(username, label)| {
            needle.is_empty()
                || username == selected
                || label.to_lowercase().contains(&needle)
                || username.to_lowercase().contains(&needle)
                || users
                    .iter()
                    .any(|u| &u.username == username && u.display_name.to_lowercase().contains(&needle))
        })
        .cloned()
        .collect()
}

/// Baut die Anfrage aus den Eingaben der Schnellerfassung. Leere Notiz und leere Auswahl entfallen.
pub fn build_swap(
    device_id: &str,
    soc: Option<f32>,
    technician: &str,
    note: &str,
    price_cents: Option<i32>,
    model: &str,
    battery_type_id: Option<i64>,
) -> BatterySwap {
    BatterySwap {
        battery_type_id,
        model: Some(model.trim().to_owned()).filter(|m| !m.is_empty()),
        device_id: device_id.to_owned(),
        note: Some(note.trim().to_owned()).filter(|n| !n.is_empty()),
        technician: Some(technician.trim().to_owned()).filter(|t| !t.is_empty()),
        soc_percent: soc,
        price_cents,
    }
}

/// Knopf, der die Schnellerfassung für ein Gerät öffnet.
#[component]
pub fn SwapButton(
    device_id: String,
    /// Kurze Form mit nur "Gewechselt" für enge Stellen.
    #[prop(optional)]
    compact: bool,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let s = expect_context::<Store>();
    let label = if compact {
        "Gewechselt"
    } else {
        "Batteriewechsel bestätigen"
    };
    view! {
        <button
            class=format!("btn btn-swap {class}")
            title="Batteriewechsel bestätigen"
            aria-label="Batteriewechsel bestätigen"
            on:click=move |ev| {
                ev.stop_propagation();
                s.modal.set(Some(ModalKind::Swap { device_id: device_id.clone() }));
            }
        >
            <Icon name="refresh-cw"/>
            <span class="btn-label">{label}</span>
        </button>
    }
}

#[component]
pub fn SwapSheet(device_id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    // Vorgabe: automatisch schätzen. Die Batterien kommen geladen zurück, niemand muss den Stand kennen.
    let soc = RwSignal::new(None::<f32>);
    // Eigener Wert: Wer den Ladestand kennt, tippt ihn ein. Leer heißt automatisch.
    let soc_text = RwSignal::new(String::new());
    let soc_error = RwSignal::new(None::<&'static str>);
    let note = RwSignal::new(String::new());
    let note_open = RwSignal::new(false);
    let price = RwSignal::new(String::new());
    s.load_battery_types();
    // Die Batterien eines Einsatzes sind meist vom selben Typ: Der zuletzt gewählte ist vorbelegt.
    let choice = RwSignal::new(crate::jsutil::storage_get(LAST_TYPE_KEY).unwrap_or_default());
    let current_choice = Memo::new(move |_| {
        let c = choice.get();
        if c == OTHER || s.battery_types.with(|t| chosen_type(&c, t).is_some()) {
            c
        } else {
            String::new()
        }
    });
    // Name und Preis werden nur frei eingetragen, wenn "Andere Batterie" gewählt ist. Bei "Keine Angabe" bleiben sie zu.
    let free_entry = Memo::new(move |_| current_choice.get() == OTHER);
    let model = RwSignal::new(crate::jsutil::storage_get(LAST_MODEL_KEY).unwrap_or_default());
    let technician = RwSignal::new(s.me.with_untracked(|m| m.as_ref().map(|m| m.username.clone()).unwrap_or_default()));
    let busy = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);
    let title = format::device_title(
        &device_id,
        s.device(&device_id).as_ref().and_then(|d| d.name.as_deref()),
    );
    let line = move || {
        let (min, max) = s.settings.with(|st| {
            st.as_ref()
                .map_or((10_500, 12_700), |st| (st.min_voltage_mv, st.max_voltage_mv))
        });
        charge_line(soc.get(), min, max)
    };
    let people = Memo::new(move |_| s.users.with(|users| short_names(users)));
    let chosen_name = Memo::new(move |_| {
        let selected = technician.get();
        people.with(|p| p.iter().find(|(u, _)| *u == selected).map(|(_, l)| l.clone()))
    });
    let submit = {
        let device_id = device_id.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            if let Some(e) = soc_error.get_untracked() {
                return problem.set(Some(e.to_owned()));
            }
            let type_id = s
                .battery_types
                .with_untracked(|t| chosen_type(&current_choice.get_untracked(), t));
            // Aus dem Katalog kommen Name und Preis vom Server, nur sonst gelten die freien Felder.
            let free = type_id.is_none() && free_entry.get_untracked();
            let price_cents = if free {
                match parse_price_cents(&price.get_untracked()) {
                    Ok(p) => p,
                    Err(e) => return problem.set(Some(e.to_owned())),
                }
            } else {
                None
            };
            busy.set(true);
            let model_text = if free { model.get_untracked() } else { String::new() };
            let swap = build_swap(
                &device_id,
                soc.get_untracked(),
                &technician.get_untracked(),
                &note.get_untracked(),
                price_cents,
                &model_text,
                type_id,
            );
            crate::jsutil::storage_set(LAST_TYPE_KEY, &current_choice.get_untracked());
            if free {
                crate::jsutil::storage_set(LAST_MODEL_KEY, swap.model.as_deref().unwrap_or_default());
            }
            spawn_local(async move {
                match api::swap_battery(&swap).await {
                    Ok(task) => {
                        s.modal.set(None);
                        s.say("Batteriewechsel gespeichert.");
                        s.offer_undo(
                            format!("Batteriewechsel bei {}", task.device_id),
                            crate::state::UndoStep::Task(task.id),
                        );
                        s.refresh_soon();
                    }
                    Err(e) => {
                        problem.set(Some(e));
                        busy.set(false);
                    }
                }
            });
        }
    };
    view! {
        <h2>"Batteriewechsel bestätigen"</h2>
        <p class="muted">{title}</p>
        <p class="sheet-line mono-num">{line}</p>
        <label class="field modal-field soc-field">
            <span>"Ladestand der neuen Batterie (leer lassen für automatisch)"</span>
            <span class="soc-box">
            <input
                type="text"
                inputmode="decimal"
                placeholder="auto"
                aria-label="Ladestand der neuen Batterie in Prozent"
                prop:value=move || soc_text.get()
                on:input=move |ev| {
                    let text = event_target_value(&ev);
                    match parse_soc(&text) {
                        Ok(value) => {
                            soc.set(value);
                            soc_error.set(None);
                        }
                        Err(e) => soc_error.set(Some(e)),
                    }
                    soc_text.set(text);
                }
            />
            <span class="soc-unit" aria-hidden="true">"%"</span>
            </span>
        </label>
        {move || soc_error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
        <p class="muted sheet-hint">
            "Die Batterie kommt geladen zurück. Ohne Angabe schätzt das System den Stand aus der ersten Messung."
        </p>
        <p class="sheet-label">"Wer hat gewechselt?"</p>
        <PersonSearch
            on_pick=Callback::new(move |username: String| technician.set(username))
            label=Signal::derive(move || chosen_name.get().unwrap_or_default())
            exclude=Signal::derive(BTreeSet::new)
            placeholder="Name tippen oder auswählen"
        />
        <label class="field modal-field">
            <span>"Welche Batterie wurde eingebaut?"</span>
            <select on:change=move |ev| choice.set(event_target_value(&ev))>
                <option value="" selected=move || current_choice.get().is_empty()>
                    "Keine Angabe"
                </option>
                {move || {
                    s.battery_types
                        .get()
                        .into_iter()
                        .map(|t| {
                            let value = t.id.to_string();
                            let mine = value.clone();
                            view! {
                                <option value=value selected=move || current_choice.get() == mine>
                                    {t.menu_text()}
                                </option>
                            }
                        })
                        .collect_view()
                }}
                <option value=OTHER selected=move || current_choice.get() == OTHER>
                    "Andere Batterie (Name und Preis eintippen)"
                </option>
            </select>
        </label>
        <Show when=move || s.battery_types.with(Vec::is_empty)>
            <p class="muted sheet-hint">
                "Der Katalog ist noch leer. Wer Batterien bestellt, legt sie unter Wartung, Batterien mit Preis an, danach reicht hier die Auswahl."
            </p>
        </Show>
        <Show when=move || free_entry.get()>
            <label class="field modal-field">
                <span>"Welche Batterie? Name eintippen (optional)"</span>
                <input
                    type="text"
                    maxlength="60"
                    placeholder="Zum Beispiel: Varta Silver 12 V 80 Ah"
                    prop:value=move || model.get()
                    on:input=move |ev| model.set(event_target_value(&ev))
                />
            </label>
            <label class="field modal-field">
                <span>"Preis in Euro (optional)"</span>
                <input
                    type="text"
                    inputmode="decimal"
                    placeholder="89,50"
                    prop:value=move || price.get()
                    on:input=move |ev| price.set(event_target_value(&ev))
                />
            </label>
        </Show>
        <button class="link sheet-more" on:click=move |_| note_open.update(|o| *o = !*o)>
            {move || if note_open.get() { "Notiz ausblenden" } else { "Notiz hinzufügen" }}
        </button>
        <Show when=move || note_open.get()>
            <textarea
                rows="2"
                maxlength="500"
                placeholder="Zum Beispiel: Pole gereinigt, Kabel getauscht"
                prop:value=move || note.get()
                on:input=move |ev| note.set(event_target_value(&ev))
            ></textarea>
        </Show>
        {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
        <div class="modal-actions sheet-actions">
            <button class="btn" on:click=move |_| s.modal.set(None)>"Abbrechen"</button>
            <button class="btn btn-primary btn-lg" disabled=move || busy.get() on:click=submit>
                "Speichern"
            </button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me(username: &str, name: &str) -> Me {
        Me {
            username: username.into(),
            display_name: name.into(),
            avatar: 0,
            admin: false,
        }
    }

    #[test]
    fn the_known_charge_can_be_typed_in_or_left_automatic() {
        assert_eq!(parse_soc(""), Ok(None));
        assert_eq!(parse_soc("   "), Ok(None));
        assert_eq!(parse_soc("95"), Ok(Some(95.0)));
        assert_eq!(parse_soc("95,5 %"), Ok(Some(95.5)));
        assert_eq!(parse_soc("87.46"), Ok(Some(87.5)));
        assert_eq!(parse_soc("0"), Ok(Some(0.0)));
        assert_eq!(parse_soc("100"), Ok(Some(100.0)));
        for bad in ["101", "-1", "abc", "NaN", "inf", "9 5"] {
            assert!(parse_soc(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_full_battery_matches_the_top_of_the_voltage_range() {
        assert_eq!(soc_voltage_mv(100.0, 10_500, 12_700), 12_700);
        assert_eq!(soc_voltage_mv(0.0, 10_500, 12_700), 10_500);
        assert_eq!(soc_voltage_mv(50.0, 10_500, 12_700), 11_600);
        assert_eq!(
            soc_voltage_mv(250.0, 10_500, 12_700),
            12_700,
            "unsinnige Werte werden begrenzt"
        );
        assert_eq!(soc_voltage_mv(-5.0, 10_500, 12_700), 10_500);
    }

    #[test]
    fn the_line_names_the_charge_or_says_it_is_estimated() {
        assert_eq!(
            charge_line(Some(100.0), 10_500, 12_700),
            "Neuer Ladestand: 100,0 % (12,70 V)"
        );
        assert_eq!(
            charge_line(Some(90.0), 10_500, 12_700),
            "Neuer Ladestand: 90,0 % (12,48 V)"
        );
        assert_eq!(
            charge_line(None, 10_500, 12_700),
            "Neuer Ladestand: wird mit der ersten Messung geschätzt"
        );
    }

    #[test]
    fn by_default_nobody_has_to_know_the_charge() {
        let swap = build_swap("TN0001", None, "noah", "", None, "", None);
        assert_eq!(swap.soc_percent, None);
        assert_eq!(swap.validate(), Ok(()));
        assert_eq!(
            build_swap("TN0001", Some(90.0), "noah", "", None, "", None).soc_percent,
            Some(90.0)
        );
    }

    #[test]
    fn empty_note_and_empty_technician_are_left_out() {
        let bare = build_swap("TN0001", None, "  ", "   ", None, "  ", None);
        assert_eq!(
            (bare.note, bare.technician, bare.price_cents, bare.model),
            (None, None, None, None)
        );
        let full = build_swap(
            "TN0001",
            Some(80.0),
            "fynn",
            "  Pole gereinigt ",
            Some(8_950),
            " Varta 80 ",
            Some(3),
        );
        assert_eq!(full.battery_type_id, Some(3));
        assert_eq!(full.model.as_deref(), Some("Varta 80"));
        assert_eq!(full.note.as_deref(), Some("Pole gereinigt"));
        assert_eq!(full.technician.as_deref(), Some("fynn"));
        assert_eq!((full.device_id.as_str(), full.price_cents), ("TN0001", Some(8_950)));
    }

    #[test]
    fn only_an_existing_catalogue_entry_counts_as_chosen() {
        let types = [shared::BatteryType {
            id: 7,
            name: "Varta".into(),
            price_cents: Some(8_950),
            note: None,
        }];
        assert_eq!(chosen_type("7", &types), Some(7));
        assert_eq!(chosen_type("8", &types), None, "gibt es nicht mehr");
        assert_eq!(chosen_type("", &types), None);
        assert_eq!(chosen_type(OTHER, &types), None);
    }

    #[test]
    fn prices_are_read_in_euro_with_comma_or_point() {
        assert_eq!(parse_price_cents(""), Ok(None));
        assert_eq!(parse_price_cents("   "), Ok(None));
        assert_eq!(parse_price_cents("89"), Ok(Some(8_900)));
        assert_eq!(parse_price_cents("89,5"), Ok(Some(8_950)));
        assert_eq!(parse_price_cents("89,50"), Ok(Some(8_950)));
        assert_eq!(parse_price_cents("89.05"), Ok(Some(8_905)));
        assert_eq!(parse_price_cents(" 120 € "), Ok(Some(12_000)));
        assert_eq!(parse_price_cents("45,90 EUR"), Ok(Some(4_590)));
        assert_eq!(parse_price_cents("0"), Ok(Some(0)));
        assert_eq!(parse_price_cents("10000"), Ok(Some(1_000_000)));
    }

    #[test]
    fn wrong_prices_are_refused_with_a_hint() {
        for bad in ["abc", "-5", "89,999", "1e3", ",50", "89,5,0", "12 34", "123456"] {
            assert!(parse_price_cents(bad).is_err(), "{bad}");
        }
        assert_eq!(
            parse_price_cents("10000,01"),
            Err("Der Preis darf höchstens 10.000 Euro betragen.")
        );
        assert!(parse_price_cents("abc").unwrap_err().contains("89,50"));
    }

    #[test]
    fn names_are_short_unless_that_is_ambiguous() {
        let users = [
            me("noah", "Noah Boufercha"),
            me("fynn", "Fynn Becker"),
            me("test", "Test"),
        ];
        let labels: Vec<String> = short_names(&users).into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, ["Noah B.", "Fynn B.", "Test"]);

        let twins = [
            me("nb", "Noah Boufercha"),
            me("nbe", "Noah Berger"),
            me("fb", "Fynn Becker"),
        ];
        let labels: Vec<String> = short_names(&twins).into_iter().map(|(_, l)| l).collect();
        assert_eq!(
            labels,
            ["Noah Boufercha", "Noah Berger", "Fynn B."],
            "gleiche Kurzform: beide mit vollem Namen"
        );

        let same = [me("a", "Noah Berger"), me("b", "Noah Berger")];
        let labels: Vec<String> = short_names(&same).into_iter().map(|(_, l)| l).collect();
        assert_eq!(
            labels,
            ["Noah Berger (a)", "Noah Berger (b)"],
            "auch der volle Name doppelt: mit Kennung"
        );

        let long = [me("x", "Anna Maria von Berg")];
        assert_eq!(short_names(&long)[0].1, "Anna B.", "der letzte Teil zählt als Nachname");
        assert!(short_names(&[]).is_empty());
    }

    #[test]
    fn the_search_finds_by_name_or_login_and_keeps_the_chosen_person() {
        let users: Vec<Me> = (0..9)
            .map(|i| me(&format!("user{i}"), &format!("Person{i} Muster")))
            .chain([me("noah", "Noah Boufercha")])
            .collect();
        let people = short_names(&users);
        assert_eq!(
            visible_people(&people, &users, "", "user0").len(),
            10,
            "ohne Suche alle"
        );
        let found = visible_people(&people, &users, "  BOUF ", "user3");
        let names: Vec<&str> = found.iter().map(|(u, _)| u.as_str()).collect();
        assert_eq!(names, ["user3", "noah"], "Treffer und die gewählte Person bleiben");
        let by_login = visible_people(&people, &users, "user7", "noah");
        assert!(by_login.iter().any(|(u, _)| u == "user7") && by_login.iter().any(|(u, _)| u == "noah"));
        let none = visible_people(&people, &users, "gibtesnicht", "");
        assert!(none.is_empty());
    }
}
