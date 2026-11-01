//! Geräteansicht mit Ladestandsverlauf (F04).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use shared::api::{DeviceSummary, History, Incident, Range, Settings};

use super::api;
use super::chart::ChargeChart;
use super::live::Live;
use super::ui::{ChargeBar, Empty, ErrorBox, Loading, StatusPill, date_time, now_ms};
use crate::format;

#[component]
pub fn DevicePage() -> impl IntoView {
    let live = expect_context::<Live>();
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    let range = RwSignal::new(Range::Day);

    let device = LocalResource::new(move || {
        live.tick.track();
        let id = id();
        async move { api::device(&id).await }
    });
    let history = LocalResource::new(move || {
        live.tick.track();
        let (id, range) = (id(), range.get());
        async move { api::history(&id, range).await }
    });
    let settings = LocalResource::new(move || {
        live.tick.track();
        api::settings()
    });
    let incidents = LocalResource::new(move || {
        live.tick.track();
        let id = id();
        async move { api::incidents("all", Some(&id)).await }
    });

    let points = Signal::derive(move || {
        history
            .get()
            .and_then(|r| {
                let r: &Result<History, String> = &r;
                r.as_ref().ok().map(|h| h.points.clone())
            })
            .unwrap_or_default()
    });
    let threshold = Signal::derive(move || {
        settings.get().and_then(|r| {
            let r: &Result<Settings, String> = &r;
            r.as_ref().ok().map(|s| s.critical_threshold_pct)
        })
    });

    view! {
        <p class="crumbs"><A href="/">"Übersicht"</A>" / "{id}</p>

        <Transition fallback=|| view! { <Loading/> }>
            {move || {
                device
                    .get()
                    .map(|result| {
                        let result: &Result<DeviceSummary, String> = &result;
                        match result {
                            Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                            Ok(d) => view! { <DeviceHeader device=d.clone()/> }.into_any(),
                        }
                    })
            }}
        </Transition>

        <section class="card">
            <div class="card-head">
                <h2>"Ladestand"</h2>
                <div class="tabs" role="tablist" aria-label="Zeitraum">
                    {Range::ALL
                        .into_iter()
                        .map(|r| {
                            view! {
                                <button
                                    role="tab"
                                    class:active=move || range.get() == r
                                    aria-selected=move || (range.get() == r).to_string()
                                    on:click=move |_| range.set(r)
                                >
                                    {format::range(r)}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
            {move || {
                history
                    .get()
                    .and_then(|r| {
                        let r: &Result<History, String> = &r;
                        r.as_ref().err().map(|e| view! { <ErrorBox message=e.clone()/> })
                    })
            }}
            <ChargeChart points threshold/>
            <p class="hint">
                {move || {
                    let n = points.get().len();
                    if n == 0 { "Für diesen Zeitraum gibt es keine Messwerte.".to_owned() } else { format!("{n} Messwerte") }
                }}
            </p>
        </section>

        <section class="card">
            <h2>"Meldungen zu diesem Gerät"</h2>
            <Transition fallback=|| view! { <Loading/> }>
                {move || {
                    incidents
                        .get()
                        .map(|result| {
                            let result: &Result<Vec<Incident>, String> = &result;
                            match result {
                                Err(message) => view! { <ErrorBox message=message.clone()/> }.into_any(),
                                Ok(list) if list.is_empty() => view! { <Empty text="Keine Meldungen."/> }.into_any(),
                                Ok(list) => view! { <IncidentList list=list.clone()/> }.into_any(),
                            }
                        })
                }}
            </Transition>
        </section>
    }
}

#[component]
fn DeviceHeader(device: DeviceSummary) -> impl IntoView {
    let age = device.last_reading_ms.map(|t| format::age(now_ms() - t));
    let exact = device.last_reading_ms.map(date_time);
    view! {
        <div class="page-head">
            <div>
                <h1>{device.id.clone()}</h1>
                <p class="sub">
                    {match (age, exact) {
                        (Some(age), Some(exact)) => format!("Letzter Messwert {age} ({exact})"),
                        _ => "Noch kein Messwert".to_owned(),
                    }}
                </p>
            </div>
            <StatusPill status=device.status/>
        </div>
        <div class="card"><ChargeBar pct=device.charge_pct status=device.status/></div>
    }
}

#[component]
fn IncidentList(list: Vec<Incident>) -> impl IntoView {
    view! {
        <ul class="plain">
            {list
                .into_iter()
                .map(|i| {
                    let closed = i.closed_at_ms.map(|t| format!(", geschlossen am {}", date_time(t))).unwrap_or_default();
                    view! {
                        <li>
                            <b>{format::incident_state(i.state)}</b>
                            {format!(" · eröffnet am {} bei {}{}", date_time(i.opened_at_ms), format::percent(i.charge_pct), closed)}
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}
