//! Geräte ausmustern: Wenn ein Gerät alles erledigt hat, ersetzt oder weggeworfen wurde, verschwindet es aus Übersicht,
//! Karte und Meldungen. Messwerte und erledigte Einsätze bleiben, und man kann es jederzeit wieder aufnehmen.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{Device, RetireDevice, RetireReason};

use crate::icons::Icon;
use crate::state::{ModalKind, Store};
use crate::{api, format, jsutil};

/// Satz zum Ausmustern: `Ersetzt, am 14.11.2026. Notiz: Neuer Node TN0420.`
pub fn retired_text(reason: Option<&str>, date: Option<String>, note: Option<&str>) -> String {
    let reason = reason
        .and_then(RetireReason::parse)
        .map_or("Ausgemustert", RetireReason::label);
    let mut text = match date {
        Some(date) => format!("{reason}, am {date}."),
        None => format!("{reason}."),
    };
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        text.push_str(&format!(" Notiz: {note}"));
    }
    text
}

/// Knopf am Gerät: öffnet die Rückfrage zum Ausmustern.
#[component]
pub fn RetireButton(device_id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <button
            class="btn"
            title="Gerät ausmustern, zum Beispiel wenn es ersetzt oder weggeworfen wurde"
            on:click=move |_| s.modal.set(Some(ModalKind::Retire { device_id: device_id.clone() }))
        >
            <Icon name="log-out"/>
            "Ausmustern"
        </button>
    }
}

/// Nimmt ein ausgemustertes Gerät wieder auf.
pub fn reactivate(s: Store, id: String) {
    spawn_local(async move {
        match api::reactivate_device(&id).await {
            Ok(_) => s.say(format!("{id} ist wieder aufgenommen.")),
            Err(e) => s.say(e),
        }
        s.refresh_soon();
    });
}

/// Hinweis oben auf der Seite eines ausgemusterten Geräts, mit dem Weg zurück.
#[component]
pub fn RetiredBanner(device: Memo<Option<Device>>) -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            device
                .get()
                .filter(|d| d.retired_at.is_some())
                .map(|d| {
                    let id = d.id.clone();
                    let text = retired_text(d.retired_reason.as_deref(), d.retired_at.map(jsutil::date_label), d.retired_note.as_deref());
                    view! {
                        <div class="diag off retired-banner" role="status">
                            <strong>"Ausgemustert"</strong>
                            <span>
                                {text}
                                " Das Gerät erscheint nicht mehr in Übersicht, Karte und Meldungen."
                            </span>
                            <button class="btn btn-sm btn-primary" on:click=move |_| reactivate(s, id.clone())>
                                "Wieder aufnehmen"
                            </button>
                        </div>
                    }
                })
        }}
    }
}

/// Rückfrage zum Ausmustern: Grund wählen, optional eine Notiz.
#[component]
pub fn RetireSheet(device_id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let reason = RwSignal::new(RetireReason::Done);
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);
    let title = format::device_title(
        &device_id,
        s.device(&device_id).as_ref().and_then(|d| d.name.as_deref()),
    );
    let submit = {
        let device_id = device_id.clone();
        move |_| {
            if busy.get_untracked() {
                return;
            }
            let request = RetireDevice {
                reason: reason.get_untracked(),
                note: Some(note.get_untracked().trim().to_owned()).filter(|n| !n.is_empty()),
            };
            if let Err(e) = request.validate() {
                return problem.set(Some(e.to_owned()));
            }
            busy.set(true);
            let id = device_id.clone();
            spawn_local(async move {
                match api::retire_device(&id, &request).await {
                    Ok(_) => {
                        s.modal.set(None);
                        s.say(format!("{id} ist ausgemustert."));
                        s.offer_undo(
                            format!("{id} ausgemustert"),
                            crate::state::UndoStep::Reactivate(id.clone()),
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
        <h2>"Gerät ausmustern?"</h2>
        <p class="muted">{title}</p>
        <p class="muted sheet-hint">
            "Das Gerät verschwindet aus Übersicht, Karte und Meldungen. Offene Meldungen werden gelöst und geplante Einsätze entfallen. Messwerte und erledigte Einsätze bleiben gespeichert. Du kannst das Gerät jederzeit wieder aufnehmen."
        </p>
        <p class="sheet-label">"Warum?"</p>
        <div class="chips" role="group" aria-label="Grund zum Ausmustern">
            {RetireReason::ALL
                .into_iter()
                .map(|r| {
                    view! {
                        <button
                            class=move || if reason.get() == r { "chip is-active" } else { "chip" }
                            aria-pressed=move || (reason.get() == r).to_string()
                            on:click=move |_| reason.set(r)
                        >
                            {r.label()}
                        </button>
                    }
                })
                .collect_view()}
        </div>
        <label class="field modal-field">
            <span>"Notiz (optional)"</span>
            <input
                type="text"
                maxlength="500"
                placeholder="Zum Beispiel: ersetzt durch TN0420"
                prop:value=move || note.get()
                on:input=move |ev| note.set(event_target_value(&ev))
            />
        </label>
        {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
        <div class="modal-actions sheet-actions">
            <button class="btn" on:click=move |_| s.modal.set(None)>"Abbrechen"</button>
            <button class="btn btn-danger btn-lg" disabled=move || busy.get() on:click=submit>
                "Gerät ausmustern"
            </button>
        </div>
    }
}

/// Liste der ausgemusterten Geräte am Ende der Übersicht. Ohne ausgemusterte Geräte ist sie nicht da.
#[component]
pub fn RetiredList() -> impl IntoView {
    let s = expect_context::<Store>();
    let open = RwSignal::new(false);
    let count = move || s.retired.with(Vec::len);
    // Der Vergleich steht außerhalb von view!, ein ">" im Attribut beendet dort das Tag.
    let any = move || count() > 0;
    view! {
        <Show when=any>
            <section class="surface surface-pad retired-list" aria-label="Ausgemusterte Geräte">
                <button class="retired-head" aria-expanded=move || open.get().to_string() on:click=move |_| open.update(|o| *o = !*o)>
                    <h2>{move || format!("Ausgemusterte Geräte ({})", count())}</h2>
                    {move || if open.get() { view! { <Icon name="chevron-up"/> }.into_any() } else { view! { <Icon name="chevron-down"/> }.into_any() }}
                </button>
                <Show when=move || open.get()>
                    <ul class="retired-items">
                        {move || {
                            s.retired
                                .get()
                                .into_iter()
                                .map(|d| {
                                    let open_id = d.id.clone();
                                    let back_id = d.id.clone();
                                    let text = retired_text(
                                        d.retired_reason.as_deref(),
                                        d.retired_at.map(jsutil::date_label),
                                        d.retired_note.as_deref(),
                                    );
                                    view! {
                                        <li class="retired-item">
                                            <button class="link retired-id mono-num" on:click=move |_| s.open_device_page(&open_id)>
                                                {d.id.clone()}
                                            </button>
                                            <span class="muted retired-why">{text}</span>
                                            <button class="btn btn-sm" on:click=move |_| reactivate(s, back_id.clone())>
                                                "Wieder aufnehmen"
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ul>
                </Show>
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_names_reason_date_and_note() {
        let text = retired_text(
            Some("replaced"),
            Some("14.11.2026".to_owned()),
            Some("  durch TN0420 ersetzt "),
        );
        assert!(text.starts_with("Ersetzt, am 14.11.2026."), "{text}");
        assert!(text.ends_with("Notiz: durch TN0420 ersetzt"), "{text}");
        assert_eq!(retired_text(Some("discarded"), None, None), "Weggeworfen.");
        assert_eq!(retired_text(None, None, Some("  ")), "Ausgemustert.");
        assert_eq!(retired_text(Some("kaputt"), None, None), "Ausgemustert.");
    }
}
