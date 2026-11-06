//! Wartung und Optionen: geplante Batteriewechsel und Schwellwerte.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{MaintenanceStatus, MaintenanceTask};

use crate::inputs::{apply_inputs, number_text};
use crate::state::Store;
use crate::{api, format, jsutil};

#[component]
fn TaskRow(task: MaintenanceTask) -> impl IntoView {
    let s = expect_context::<Store>();
    let device_id = task.device_id.clone();
    let title = {
        let device_id = device_id.clone();
        move || {
            format::device_title(
                &device_id,
                s.device(&device_id).as_ref().and_then(|d| d.name.as_deref()),
            )
        }
    };
    let open = {
        let device_id = device_id.clone();
        move |_| s.selected.set(Some(device_id.clone()))
    };
    let done = task.status == MaintenanceStatus::Completed;
    let task_id = task.id;
    let confirm = move |_| {
        spawn_local(async move {
            match api::complete_task(task_id).await {
                Ok(_) => s.say("Batteriewechsel bestätigt."),
                Err(e) => s.say(e),
            }
            s.refresh_soon();
        });
    };
    view! {
        <li class="task-row">
            <button class="link" on:click=open>{title}</button>
            <span class="muted">
                {if done {
                    format!("Erledigt am {}", task.completed_at.map(jsutil::date_label).unwrap_or_default())
                } else {
                    format!("Geplant seit {}", jsutil::date_label(task.created_at))
                }}
            </span>
            {task.note.clone().map(|n| view! { <span class="note">{n}</span> })}
            {(!done).then(|| view! { <button class="btn primary" on:click=confirm>"Batterie gewechselt"</button> })}
        </li>
    }
}

#[component]
fn Tasks() -> impl IntoView {
    let s = expect_context::<Store>();
    let planned = move || {
        s.tasks.with(|t| {
            t.iter()
                .filter(|t| t.status == MaintenanceStatus::Planned)
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    let completed = move || {
        s.tasks.with(|t| {
            t.iter()
                .filter(|t| t.status == MaintenanceStatus::Completed)
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
        })
    };
    view! {
        <section class="panel">
            <h2>"Geplante Batteriewechsel"</h2>
            {move || {
                let list = planned();
                if list.is_empty() {
                    view! { <p class="muted">"Keine geplanten Einsätze. Wechsel lassen sich in der Übersicht oder in den Gerätedetails planen."</p> }.into_any()
                } else {
                    view! {
                        <ul class="tasks">
                            <For each=planned key=|t| (t.id, t.status) children=move |t| view! { <TaskRow task=t/> }/>
                        </ul>
                    }
                        .into_any()
                }
            }}
            {move || {
                (!completed().is_empty())
                    .then(|| {
                        view! {
                            <h3>"Zuletzt erledigt"</h3>
                            <ul class="tasks">
                                <For each=completed key=|t| (t.id, t.status) children=move |t| view! { <TaskRow task=t/> }/>
                            </ul>
                        }
                    })
            }}
        </section>
    }
}

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
        <section class="panel">
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
pub fn Maintenance() -> impl IntoView {
    view! {
        <header class="page-head">
            <h1>"Wartung & Optionen"</h1>
            <p class="sub">"Batteriewechsel verwalten und Schwellwerte einstellen."</p>
        </header>
        <div class="two-col">
            <Tasks/>
            <Thresholds/>
        </div>
    }
}
