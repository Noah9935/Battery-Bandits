//! Optionen: Schwellwert (F05, NF07) und das Protokoll verworfener Nachrichten (F06).

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{RejectedMessage, Settings};

use super::api;
use super::live::Live;
use super::ui::{Empty, ErrorBox, Loading, date_time};
use crate::format;

#[component]
pub fn SettingsPage() -> impl IntoView {
    let live = expect_context::<Live>();
    let settings = LocalResource::new(move || {
        live.tick.track();
        api::settings()
    });
    let rejected = LocalResource::new(move || {
        live.tick.track();
        api::rejected()
    });

    view! {
        <div class="page-head">
            <div>
                <h1>"Optionen"</h1>
                <p class="sub">"Einstellungen wirken sofort und brauchen keine Codeänderung."</p>
            </div>
        </div>

        <section class="card">
            <h2>"Warnschwelle"</h2>
            <p class="hint">"Geräte unter diesem Ladestand gelten als kritisch."</p>
            <Transition fallback=|| view! { <Loading/> }>
                {move || {
                    settings
                        .get()
                        .map(|result| {
                            let result: &Result<Settings, String> = &result;
                            match result {
                                Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                                Ok(s) => view! { <ThresholdForm current=s.critical_threshold_pct/> }.into_any(),
                            }
                        })
                }}
            </Transition>
        </section>

        <section class="card">
            <h2>"Verworfene Nachrichten"</h2>
            <p class="hint">"Nachrichten, die nicht dem erwarteten Format entsprechen, werden verworfen und hier protokolliert."</p>
            <Transition fallback=|| view! { <Loading/> }>
                {move || {
                    rejected
                        .get()
                        .map(|result| {
                            let result: &Result<Vec<RejectedMessage>, String> = &result;
                            match result {
                                Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                                Ok(list) if list.is_empty() => view! { <Empty text="Es wurde noch nichts verworfen."/> }.into_any(),
                                Ok(list) => view! { <RejectedTable list=list.clone()/> }.into_any(),
                            }
                        })
                }}
            </Transition>
        </section>
    }
}

#[component]
fn ThresholdForm(current: f64) -> impl IntoView {
    let value = RwSignal::new(format::number(current, 0));
    let feedback = RwSignal::new(None::<Result<String, String>>);
    let saving = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Ok(number) = value.get().trim().replace(',', ".").parse::<f64>() else {
            feedback.set(Some(Err("Bitte eine Zahl eingeben.".to_owned())));
            return;
        };
        saving.set(true);
        spawn_local(async move {
            let result = api::save_settings(&Settings {
                critical_threshold_pct: number,
            })
            .await;
            feedback.set(Some(result.map(|s| {
                format!(
                    "Gespeichert: unter {} gilt ein Gerät als kritisch.",
                    format::percent(s.critical_threshold_pct)
                )
            })));
            saving.set(false);
        });
    };

    view! {
        <form class="inline-form" on:submit=submit>
            <label for="threshold">"Schwellwert in Prozent"</label>
            <input
                id="threshold"
                type="text"
                inputmode="decimal"
                prop:value=move || value.get()
                on:input=move |ev| value.set(event_target_value(&ev))
            />
            <button class="btn primary" type="submit" disabled=move || saving.get()>"Speichern"</button>
        </form>
        {move || {
            feedback
                .get()
                .map(|f| match f {
                    Ok(text) => view! { <div class="notice" role="status">{text}</div> }.into_any(),
                    Err(text) => view! { <ErrorBox message=text/> }.into_any(),
                })
        }}
    }
}

#[component]
fn RejectedTable(list: Vec<RejectedMessage>) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Zeitpunkt"</th>
                        <th>"Topic"</th>
                        <th>"Grund"</th>
                        <th>"Inhalt"</th>
                    </tr>
                </thead>
                <tbody>
                    {list
                        .into_iter()
                        .map(|m| {
                            view! {
                                <tr>
                                    <td>{date_time(m.received_at_ms)}</td>
                                    <td><code>{m.topic}</code></td>
                                    <td>{m.reason}</td>
                                    <td><code class="payload">{m.payload}</code></td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
}
