//! Meldungen zu kritischen Geräten (Event Storming 7 bis 9).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use shared::api::{Incident, IncidentState};

use super::api;
use super::live::Live;
use super::ui::{Empty, ErrorBox, Loading, date_time};
use crate::format;

#[component]
pub fn IncidentsPage() -> impl IntoView {
    let live = expect_context::<Live>();
    let only_open = RwSignal::new(true);
    let feedback = RwSignal::new(None::<Result<String, String>>);

    let incidents = LocalResource::new(move || {
        live.tick.track();
        let state = if only_open.get() { "open" } else { "all" };
        async move { api::incidents(state, None).await }
    });

    let dismiss = move |id: i64| {
        spawn_local(async move {
            match api::dismiss_incident(id).await {
                Ok(()) => feedback.set(Some(Ok("Die Meldung wurde abgewiesen.".to_owned()))),
                Err(e) => feedback.set(Some(Err(e))),
            }
            incidents.refetch();
        });
    };

    view! {
        <div class="page-head">
            <div>
                <h1>"Meldungen"</h1>
                <p class="sub">
                    "Wird ein Gerät kritisch, eröffnet das System eine Meldung. Wird es wieder normal, schließt sie sich selbst."
                </p>
            </div>
        </div>

        <div class="toolbar">
            <label class="check">
                <input type="checkbox" prop:checked=move || only_open.get() on:change=move |ev| only_open.set(event_target_checked(&ev))/>
                "Nur offene Meldungen"
            </label>
        </div>

        {move || {
            feedback
                .get()
                .map(|f| match f {
                    Ok(text) => view! { <div class="notice" role="status">{text}</div> }.into_any(),
                    Err(text) => view! { <ErrorBox message=text/> }.into_any(),
                })
        }}

        <Transition fallback=|| view! { <Loading/> }>
            {move || {
                incidents
                    .get()
                    .map(|result| {
                        let result: &Result<Vec<Incident>, String> = &result;
                        match result {
                            Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                            Ok(list) if list.is_empty() => view! { <Empty text="Keine Meldungen. Alle Geräte sind in Ordnung."/> }.into_any(),
                            Ok(list) => view! { <IncidentTable list=list.clone() dismiss=dismiss/> }.into_any(),
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn IncidentTable(list: Vec<Incident>, dismiss: impl Fn(i64) + Copy + Send + Sync + 'static) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Gerät"</th>
                        <th>"Status"</th>
                        <th>"Ladestand bei Eröffnung"</th>
                        <th>"Eröffnet"</th>
                        <th>"Geschlossen"</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    {list
                        .into_iter()
                        .map(|i| {
                            let id = i.id;
                            let open = i.state == IncidentState::Open;
                            view! {
                                <tr>
                                    <td><A href=format!("/geraete/{}", i.device_id)>{i.device_id.clone()}</A></td>
                                    <td>{format::incident_state(i.state)}</td>
                                    <td>{format::percent(i.charge_pct)}</td>
                                    <td>{date_time(i.opened_at_ms)}</td>
                                    <td>{i.closed_at_ms.map(date_time).unwrap_or_else(|| "offen".to_owned())}</td>
                                    <td class="right">
                                        {open
                                            .then(|| {
                                                view! {
                                                    <button class="btn" on:click=move |_| dismiss(id)>"Abweisen"</button>
                                                }
                                            })}
                                    </td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
    }
}
