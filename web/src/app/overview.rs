//! Geräteübersicht (F03, F05, F13, NF04).

use leptos::prelude::*;
use leptos_router::components::A;
use shared::api::{Counts, DeviceSummary};

use super::api;
use super::live::Live;
use super::ui::{ChargeBar, Empty, ErrorBox, Loading, StatusPill, now_ms};
use crate::filter::{self, SortKey, StatusFilter};
use crate::format;

#[component]
pub fn OverviewPage() -> impl IntoView {
    let live = expect_context::<Live>();
    let devices = LocalResource::new(move || {
        live.tick.track();
        api::devices()
    });

    let query = RwSignal::new(String::new());
    let status_filter = RwSignal::new(StatusFilter::All);
    let sort = RwSignal::new(SortKey::CriticalFirst);

    view! {
        <div class="page-head">
            <div>
                <h1>"Geräteübersicht"</h1>
                <p class="sub">"Batteriezustand aller TrafficNodes. Kritische Geräte stehen oben."</p>
            </div>
        </div>

        // Die Bedienelemente stehen außerhalb der Datenansicht, damit sie beim Aktualisieren nicht neu aufgebaut werden.
        <div class="toolbar">
            <input
                type="search"
                placeholder="Gerät suchen, zum Beispiel TN-0042"
                aria-label="Gerät suchen"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            <select
                aria-label="Nach Status filtern"
                on:change=move |ev| status_filter.set(StatusFilter::from_key(&event_target_value(&ev)))
            >
                {StatusFilter::ALL
                    .into_iter()
                    .map(|f| view! { <option value=f.key() selected=move || status_filter.get() == f>{f.label()}</option> })
                    .collect_view()}
            </select>
            <select aria-label="Sortierung" on:change=move |ev| sort.set(SortKey::from_key(&event_target_value(&ev)))>
                {SortKey::ALL
                    .into_iter()
                    .map(|s| view! { <option value=s.key() selected=move || sort.get() == s>{s.label()}</option> })
                    .collect_view()}
            </select>
        </div>

        <Transition fallback=|| view! { <Loading/> }>
            {move || {
                devices
                    .get()
                    .map(|result| {
                        let result: &Result<Vec<DeviceSummary>, String> = &result;
                        match result {
                            Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                            Ok(list) if list.is_empty() => {
                                view! {
                                    <Empty text="Noch keine Geräte. Sobald das erste TrafficNode einen Messwert sendet, erscheint es hier."/>
                                }
                                    .into_any()
                            }
                            Ok(list) => {
                                view! { <Summary counts=Counts::of(list)/><DeviceTable list=list.clone() query status_filter sort/> }
                                    .into_any()
                            }
                        }
                    })
            }}
        </Transition>
    }
}

#[component]
fn Summary(counts: Counts) -> impl IntoView {
    view! {
        <div class="stats">
            <div class="stat"><span>"Geräte"</span><b>{counts.total}</b></div>
            <div class="stat crit"><span>"Kritisch"</span><b>{counts.critical}</b></div>
            <div class="stat ok"><span>"In Ordnung"</span><b>{counts.ok}</b></div>
        </div>
    }
}

#[component]
fn DeviceTable(
    list: Vec<DeviceSummary>,
    query: RwSignal<String>,
    status_filter: RwSignal<StatusFilter>,
    sort: RwSignal<SortKey>,
) -> impl IntoView {
    let rows = Memo::new(move |_| filter::apply(&list, &query.get(), status_filter.get(), sort.get()));
    let shown = move || rows.with(|r| r.len());
    let now = now_ms();

    view! {
        <p class="hint" aria-live="polite">{move || format!("{} Geräte angezeigt", shown())}</p>
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Gerät"</th>
                        <th>"Ladestand"</th>
                        <th>"Status"</th>
                        <th>"Letzter Messwert"</th>
                        <th>"Meldung"</th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || rows.get()
                        key=|d| format!("{}|{:?}|{:?}|{}", d.id, d.last_reading_ms, d.status, d.open_incident)
                        children=move |d| {
                            let href = format!("/geraete/{}", d.id);
                            view! {
                                <tr class=if d.status == shared::Status::Critical { "row crit" } else { "row" }>
                                    <td><A href=href>{d.id.clone()}</A></td>
                                    <td><ChargeBar pct=d.charge_pct status=d.status/></td>
                                    <td><StatusPill status=d.status/></td>
                                    <td>{d.last_reading_ms.map(|t| format::age(now - t)).unwrap_or_else(|| "nie".to_owned())}</td>
                                    <td>
                                        {if d.open_incident {
                                            view! { <A href="/meldungen"><span class="badge">"Offen"</span></A> }.into_any()
                                        } else {
                                            view! { <span class="muted">"keine"</span> }.into_any()
                                        }}
                                    </td>
                                </tr>
                            }
                        }
                    />
                </tbody>
            </table>
        </div>
    }
}
