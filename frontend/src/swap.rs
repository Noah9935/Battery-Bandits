//! Schnellerfassung des Batteriewechsels: ein Tipp auf den Knopf, ein Tipp auf "Speichern". Gerät, Datum und
//! voller Ladestand sind schon eingetragen. Wer gewechselt hat und eine Notiz lassen sich bei Bedarf ändern.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::BatterySwap;

use crate::icons::Icon;
use crate::state::{ModalKind, Store};
use crate::{api, format};

/// Ladestände, die man mit einem Tipp wählt. Der erste ist die Vorgabe.
pub const PRESETS: [f32; 3] = [100.0, 90.0, 80.0];

/// Spannung einer Batterie mit diesem Ladestand (linear zwischen leer und voll, wie bei der Schätzung des Startwerts).
pub fn soc_voltage_mv(soc: f32, min_mv: i32, max_mv: i32) -> i32 {
    let soc = f64::from(soc.clamp(0.0, 100.0));
    (f64::from(min_mv) + (f64::from(max_mv) - f64::from(min_mv)) * soc / 100.0).round() as i32
}

/// Zeile unter der Auswahl: `Neuer Ladestand: 100 % (12,70 V)`.
pub fn charge_line(soc: f32, min_mv: i32, max_mv: i32) -> String {
    format!(
        "Neuer Ladestand: {} ({})",
        format::percent(soc),
        format::voltage(soc_voltage_mv(soc, min_mv, max_mv))
    )
}

/// Baut die Anfrage aus den Eingaben der Schnellerfassung. Leere Notiz und leere Auswahl entfallen.
pub fn build_swap(device_id: &str, soc: f32, technician: &str, note: &str) -> BatterySwap {
    BatterySwap {
        device_id: device_id.to_owned(),
        note: Some(note.trim().to_owned()).filter(|n| !n.is_empty()),
        technician: Some(technician.trim().to_owned()).filter(|t| !t.is_empty()),
        soc_percent: Some(soc),
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
            on:click=move |ev| {
                ev.stop_propagation();
                s.modal.set(Some(ModalKind::Swap { device_id: device_id.clone() }));
            }
        >
            <Icon name="zap"/>
            {label}
        </button>
    }
}

#[component]
pub fn SwapSheet(device_id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let soc = RwSignal::new(PRESETS[0]);
    let note = RwSignal::new(String::new());
    let note_open = RwSignal::new(false);
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
    let submit = {
        let device_id = device_id.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            busy.set(true);
            let swap = build_swap(
                &device_id,
                soc.get_untracked(),
                &technician.get_untracked(),
                &note.get_untracked(),
            );
            spawn_local(async move {
                match api::swap_battery(&swap).await {
                    Ok(_) => {
                        s.modal.set(None);
                        s.say("Batteriewechsel gespeichert.");
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
        <div class="chips" role="group" aria-label="Ladestand der neuen Batterie">
            {PRESETS
                .into_iter()
                .map(|value| {
                    view! {
                        <button
                            class=move || if soc.get() == value { "chip is-active" } else { "chip" }
                            aria-pressed=move || (soc.get() == value).to_string()
                            on:click=move |_| soc.set(value)
                        >
                            {format!("{value:.0} %")}
                        </button>
                    }
                })
                .collect_view()}
        </div>
        <p class="sheet-label">"Wer hat gewechselt?"</p>
        <div class="chips" role="group" aria-label="Techniker">
            {move || {
                s.users
                    .get()
                    .into_iter()
                    .map(|user| {
                        let name = user.username.clone();
                        let click_name = user.username.clone();
                        view! {
                            <button
                                class=move || if technician.get() == name { "chip is-active" } else { "chip" }
                                on:click=move |_| technician.set(click_name.clone())
                            >
                                {user.display_name}
                            </button>
                        }
                    })
                    .collect_view()
            }}
        </div>
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
    fn the_line_names_charge_and_voltage() {
        assert_eq!(charge_line(100.0, 10_500, 12_700), "Neuer Ladestand: 100,0 % (12,70 V)");
        assert_eq!(charge_line(90.0, 10_500, 12_700), "Neuer Ladestand: 90,0 % (12,48 V)");
    }

    #[test]
    fn the_default_is_a_full_battery() {
        assert_eq!(PRESETS[0], 100.0);
        let swap = build_swap("TN0001", PRESETS[0], "noah", "");
        assert_eq!(swap.soc_percent, Some(100.0));
        assert_eq!(swap.validate(), Ok(()));
    }

    #[test]
    fn empty_note_and_empty_technician_are_left_out() {
        let bare = build_swap("TN0001", 90.0, "  ", "   ");
        assert_eq!((bare.note, bare.technician), (None, None));
        let full = build_swap("TN0001", 80.0, "fynn", "  Pole gereinigt ");
        assert_eq!(full.note.as_deref(), Some("Pole gereinigt"));
        assert_eq!(full.technician.as_deref(), Some("fynn"));
        assert_eq!(full.device_id, "TN0001");
    }
}
