//! Übersicht: Kennzahlen, offene Meldungen, Filter und Gerätetabelle.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{DeviceSort, SortDir};
use shared::{Alert, AlertKind, DataSource, Device, DeviceStatus, NewMaintenanceTask};

use crate::icons::Icon;
use crate::state::Store;
use crate::{api, filter, format};

/// Status als kleiner farbiger Punkt mit Text in normaler Textfarbe, nie nur Farbe.
#[component]
pub fn StatusDot(status: DeviceStatus) -> impl IntoView {
    view! {
        <span class="status">
            <span class=format!("dot {}", format::status_class(status))></span>
            {format::status_label(status)}
        </span>
    }
}

/// Herkunft der Daten: echt vom Dozenten oder erfunden (Simulator).
#[component]
pub fn Origin(source: DataSource) -> impl IntoView {
    view! {
        <span
            class=format!("origin {}", source.as_str())
            title=match source {
                DataSource::Dozent => "Echte Daten, vom Dozenten (Messdatenplattform oder Vorlesung)",
                DataSource::Simulation => "Erfundene Daten aus dem Simulator",
            }
        >
            {format::source_label(source)}
        </span>
    }
}

#[component]
fn Stat(label: &'static str, status: Option<DeviceStatus>, count: Signal<i64>) -> impl IntoView {
    let s = expect_context::<Store>();
    let active = move || s.filter.with(|f| f.status == status);
    view! {
        <button
            class=move || if active() { "stat active" } else { "stat" }
            aria-pressed=move || active().to_string()
            on:click=move |_| {
                s.filter.update(|f| f.status = if f.status == status { None } else { status });
            }
        >
            <span class="stat-label">
                {status.map(|st| view! { <span class=format!("dot {}", format::status_class(st))></span> })}
                {label}
            </span>
            <span class="stat-count">{move || count.get()}</span>
        </button>
    }
}

#[component]
fn StatStrip() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="stats" aria-label="Kennzahlen">
            <Stat label="Alle Geräte" status=None count=Signal::derive(move || s.summary.get().total)/>
            <Stat label="Kritisch" status=Some(DeviceStatus::Critical) count=Signal::derive(move || s.summary.get().critical)/>
            <Stat label="Warnung" status=Some(DeviceStatus::Warning) count=Signal::derive(move || s.summary.get().warning)/>
            <Stat label="Offline" status=Some(DeviceStatus::Offline) count=Signal::derive(move || s.summary.get().offline)/>
        </section>
    }
}

fn alert_detail(alert: &Alert, device: Option<&Device>, now_ms: f64) -> String {
    let soc = device.and_then(|d| d.soc_percent).map(format::percent);
    let seen = device
        .and_then(|d| d.last_seen_at)
        .map(|t| format::age((now_ms - t.timestamp_millis() as f64) as i64));
    match alert.kind {
        AlertKind::Critical => match soc {
            Some(soc) => format!("Ladestand {soc}"),
            None => "Ladestand kritisch".to_owned(),
        },
        AlertKind::Offline => match seen {
            Some(seen) => format!("Letzte Meldung {seen}"),
            None => "Keine Messwerte".to_owned(),
        },
    }
}

#[component]
fn AlertRow(alert: Alert) -> impl IntoView {
    let s = expect_context::<Store>();
    let device_id = alert.device_id.clone();
    let alert_id = alert.id;
    let kind = alert.kind;
    let title = {
        let device_id = device_id.clone();
        move || {
            let device = s.device(&device_id);
            format::device_title(&device_id, device.as_ref().and_then(|d| d.name.as_deref()))
        }
    };
    let detail = {
        let device_id = device_id.clone();
        let alert = alert.clone();
        move || alert_detail(&alert, s.device(&device_id).as_ref(), s.now_ms.get())
    };
    let planned = {
        let device_id = device_id.clone();
        move || s.planned_task(&device_id).is_some()
    };
    let open = {
        let device_id = device_id.clone();
        move |_| s.selected.set(Some(device_id.clone()))
    };
    let plan = {
        let device_id = device_id.clone();
        move |_| {
            let new = NewMaintenanceTask {
                device_id: device_id.clone(),
                alert_id: Some(alert_id),
                note: None,
            };
            spawn_local(async move {
                match api::plan_task(&new).await {
                    Ok(_) => s.say("Batteriewechsel geplant."),
                    Err(e) => s.say(e),
                }
                s.refresh_soon();
            });
        }
    };
    let dismiss = move |_| {
        spawn_local(async move {
            match api::dismiss_alert(alert_id).await {
                Ok(_) => s.say("Meldung abgewiesen."),
                Err(e) => s.say(e),
            }
            s.refresh_soon();
        });
    };
    view! {
        <li class="alert-row">
            <span class="status">
                <span class=format!("dot {}", if kind == AlertKind::Critical { "critical" } else { "offline" })></span>
                {format::alert_kind_label(kind)}
            </span>
            <button class="link mono" on:click=open>{title}</button>
            <span class="alert-detail muted">{detail}</span>
            <span class="alert-actions">
                {move || {
                    if planned() {
                        view! { <span class="muted">"Wechsel geplant"</span> }.into_any()
                    } else {
                        view! { <button class="btn primary" on:click=plan.clone()>"Batteriewechsel planen"</button> }.into_any()
                    }
                }}
                <button class="btn" on:click=dismiss>"Abweisen"</button>
            </span>
        </li>
    }
}

#[component]
fn AlertsBox() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            let alerts = s.open_alerts();
            if alerts.is_empty() {
                return view! { <p class="calm">"Keine offenen Meldungen."</p> }.into_any();
            }
            view! {
                <section class="panel alerts" aria-label="Offene Meldungen">
                    <h2>{format!("Offene Meldungen ({})", alerts.len())}</h2>
                    <ul>
                        <For
                            each=move || s.open_alerts()
                            key=|a| (a.id, a.status)
                            children=move |alert| view! { <AlertRow alert=alert/> }
                        />
                    </ul>
                </section>
            }
            .into_any()
        }}
    }
}

fn sort_value(sort: DeviceSort) -> &'static str {
    match sort {
        DeviceSort::Id => "id",
        DeviceSort::SocPercent => "soc_percent",
        DeviceSort::LastSeenAt => "last_seen_at",
        DeviceSort::Status => "status",
    }
}

fn parse_sort(value: &str) -> DeviceSort {
    match value {
        "soc_percent" => DeviceSort::SocPercent,
        "last_seen_at" => DeviceSort::LastSeenAt,
        "status" => DeviceSort::Status,
        _ => DeviceSort::Id,
    }
}

fn parse_status(value: &str) -> Option<DeviceStatus> {
    value.parse().ok()
}

fn parse_source(value: &str) -> Option<DataSource> {
    value.parse().ok()
}

#[component]
fn FilterBar(shown: Signal<usize>) -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="toolbar" aria-label="Filter">
            <label class="search">
                <Icon name="search"/>
                <input
                    type="search"
                    placeholder="Gerät suchen"
                    aria-label="Gerät suchen"
                    prop:value=move || s.filter.with(|f| f.search.clone())
                    on:input=move |ev| s.filter.update(|f| f.search = event_target_value(&ev))
                />
            </label>
            <label class="field">
                <span>"Status"</span>
                <select on:change=move |ev| s.filter.update(|f| f.status = parse_status(&event_target_value(&ev)))>
                    <option value="" selected=move || s.filter.with(|f| f.status.is_none())>"Alle"</option>
                    {DeviceStatus::ALL
                        .iter()
                        .map(|st| {
                            let st = *st;
                            view! {
                                <option value=st.as_str() selected=move || s.filter.with(|f| f.status == Some(st))>
                                    {format::status_label(st)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="field">
                <span>"Herkunft"</span>
                <select on:change=move |ev| s.filter.update(|f| f.source = parse_source(&event_target_value(&ev)))>
                    <option value="" selected=move || s.filter.with(|f| f.source.is_none())>"Alle"</option>
                    {DataSource::ALL
                        .iter()
                        .map(|src| {
                            let src = *src;
                            view! {
                                <option value=src.as_str() selected=move || s.filter.with(|f| f.source == Some(src))>
                                    {format::source_label(src)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </label>
            <label class="field">
                <span>"Sortieren nach"</span>
                <select on:change=move |ev| s.filter.update(|f| f.sort = parse_sort(&event_target_value(&ev)))>
                    {[
                        (DeviceSort::Id, "Kennung"),
                        (DeviceSort::SocPercent, "Ladestand"),
                        (DeviceSort::LastSeenAt, "Letzte Meldung"),
                        (DeviceSort::Status, "Status"),
                    ]
                        .into_iter()
                        .map(|(sort, label)| {
                            view! {
                                <option value=sort_value(sort) selected=move || s.filter.with(|f| f.sort == sort)>
                                    {label}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            </label>
            <button
                class="btn icon-only"
                aria-label="Sortierrichtung umkehren"
                title=move || if s.filter.with(|f| f.dir == SortDir::Asc) { "Aufsteigend" } else { "Absteigend" }
                on:click=move |_| {
                    s.filter.update(|f| f.dir = if f.dir == SortDir::Asc { SortDir::Desc } else { SortDir::Asc });
                }
            >
                {move || {
                    if s.filter.with(|f| f.dir == SortDir::Asc) {
                        view! { <Icon name="arrow-up"/> }
                    } else {
                        view! { <Icon name="arrow-down"/> }
                    }
                }}
            </button>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || s.filter.with(|f| f.attention_first)
                    on:change=move |ev| s.filter.update(|f| f.attention_first = event_target_checked(&ev))
                />
                <span>"Kritische zuerst"</span>
            </label>
            <span class="count tabular">
                {move || format!("{} von {} Geräten", shown.get(), s.summary.get().total)}
            </span>
        </section>
    }
}

fn row_key(d: &Device) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        d.id, d.status, d.soc_percent, d.last_seen_at, d.name, d.lat, d.data_source
    )
}

#[component]
fn DeviceRow(device: Device) -> impl IntoView {
    let s = expect_context::<Store>();
    let id = device.id.clone();
    let open = {
        let id = id.clone();
        move || s.selected.set(Some(id.clone()))
    };
    let open_click = open.clone();
    let soc = device.soc_percent;
    let width = soc.map_or(0.0, |v| f64::from(v.clamp(0.0, 100.0)));
    let seen = device.last_seen_at;
    let position = match (device.lat, device.lon) {
        (Some(lat), Some(lon)) => {
            view! { <span class="mono">{format!("{}, {}", format::coordinate(lat), format::coordinate(lon))}</span> }
                .into_any()
        }
        _ => view! { <span class="muted">"Kein GPS-Fix"</span> }.into_any(),
    };
    view! {
        <tr
            class="row"
            tabindex="0"
            on:click=move |_| open_click()
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.key() == " " {
                    ev.prevent_default();
                    open();
                }
            }
        >
            <td data-label="Status"><StatusDot status=device.status/></td>
            <td data-label="Gerät">
                <div class="dev-id mono">{device.id.clone()}</div>
                <div class="dev-meta">
                    <Origin source=device.data_source/>
                    <span class="muted">{device.name.clone().unwrap_or_default()}</span>
                </div>
            </td>
            <td data-label="Ladestand">
                <div class="soc">
                    <div class="bar" aria-hidden="true">
                        <div class="bar-fill" style:width=format!("{width:.0}%")></div>
                    </div>
                    <div class="soc-text tabular">
                        <strong>{soc.map_or_else(|| "unbekannt".to_owned(), format::percent)}</strong>
                        <span class="muted mono">{device.voltage_mv.map(format::voltage).unwrap_or_default()}</span>
                    </div>
                </div>
            </td>
            <td data-label="Strom und Temperatur">
                <div class="mono tabular">{device.current_ma.map(format::current).unwrap_or_default()}</div>
                <div class="muted mono tabular">{device.temperature_c.map(format::temperature).unwrap_or_default()}</div>
            </td>
            <td data-label="Position">{position}</td>
            <td data-label="Letzte Meldung" class="tabular">
                {move || match seen {
                    Some(t) => format::age((s.now_ms.get() - t.timestamp_millis() as f64) as i64),
                    None => "noch keine".to_owned(),
                }}
            </td>
        </tr>
    }
}

#[component]
fn DeviceTable(rows: Memo<Vec<Device>>) -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            if rows.with(|r| r.is_empty()) {
                let text = if s.devices.with(|d| d.is_empty()) {
                    if s.loaded.get() {
                        "Noch keine Messwerte empfangen. Sobald TrafficNodes senden, erscheinen sie hier."
                    } else {
                        "Daten werden geladen."
                    }
                } else {
                    "Kein Gerät passt zu den Filtern."
                };
                return view! { <p class="empty">{text}</p> }.into_any();
            }
            view! {
                <div class="panel table-wrap">
                    <table class="devices">
                        <thead>
                            <tr>
                                <th>"Status"</th>
                                <th>"Gerät"</th>
                                <th>"Ladestand"</th>
                                <th>"Strom und Temperatur"</th>
                                <th>"Position"</th>
                                <th>"Letzte Meldung"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || rows.get()
                                key=row_key
                                children=move |device| view! { <DeviceRow device=device/> }
                            />
                        </tbody>
                    </table>
                </div>
            }
            .into_any()
        }}
    }
}

#[component]
pub fn Overview() -> impl IntoView {
    let s = expect_context::<Store>();
    let rows = Memo::new(move |_| {
        let filter = s.filter.get();
        s.devices.with(|devices| filter::apply(devices, &filter))
    });
    let shown = Signal::derive(move || rows.with(Vec::len));
    let origin_text = Signal::derive(move || {
        let (real, invented) = s.devices.with(|d| filter::count_by_source(d));
        format::source_summary(real, invented)
    });
    view! {
        <header class="page-head">
            <h1>"Übersicht"</h1>
            <p class="sub">"Ladezustand aller TrafficNodes. " {move || origin_text.get()}</p>
        </header>
        <StatStrip/>
        <AlertsBox/>
        <FilterBar shown=shown/>
        <DeviceTable rows=rows/>
    }
}
