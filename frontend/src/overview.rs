//! Übersicht: Kennzahlen, offene Meldungen, Filter und Gerätetabelle.

use leptos::prelude::*;
use shared::api::{DeviceSort, SortDir};
use shared::{Alert, AlertKind, DataSource, Device, DeviceStatus};

use crate::features::Feature;
use crate::icons::Icon;
use crate::overlays::export_filtered;
use crate::state::{ModalKind, Store};
use crate::{filter, format, insights};

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
        AlertKind::Fault => match (soc, seen) {
            (Some(soc), Some(seen)) => format!("Zuletzt {soc} geladen, Meldung {seen}"),
            _ => "Vermutlich Funk oder Technik".to_owned(),
        },
        AlertKind::Stagnation => "Spannung und Strom ändern sich nicht, Sensor prüfen".to_owned(),
        AlertKind::Drop => "Spannung ist binnen kurzer Zeit stark gefallen".to_owned(),
    }
}

/// Farbpunkt der Meldung: Auffälligkeiten sind Warnungen, Ausfälle grau, leere Batterien rot.
fn alert_dot(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Critical => "critical",
        AlertKind::Offline | AlertKind::Fault => "offline",
        AlertKind::Stagnation | AlertKind::Drop => "warning",
    }
}

/// Aufschrift des Plan-Knopfs: Bei leerer Batterie ein Wechsel, sonst erst eine Kontrolle.
fn plan_label(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Critical | AlertKind::Offline => "Batteriewechsel planen",
        AlertKind::Stagnation | AlertKind::Drop | AlertKind::Fault => "Kontrolle planen",
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
            s.modal.set(Some(ModalKind::Plan {
                device_id: device_id.clone(),
                alert_id: Some(alert_id),
            }))
        }
    };
    let dismiss = {
        let device_id = device_id.clone();
        move |_| {
            s.modal.set(Some(ModalKind::Dismiss {
                alert_id,
                device_id: device_id.clone(),
            }))
        }
    };
    view! {
        <li class="alert-row">
            <span class="status">
                <span class=format!("dot {}", alert_dot(kind))></span>
                {format::alert_kind_label(kind)}
            </span>
            <button class="link mono" on:click=open>{title}</button>
            <span class="alert-detail muted">{detail}</span>
            <span class="alert-actions">
                {move || {
                    if planned() {
                        view! { <span class="muted">"Wechsel geplant"</span> }.into_any()
                    } else {
                        view! { <button class="btn primary" on:click=plan.clone()>{plan_label(kind)}</button> }.into_any()
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
                    id="search-input"
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
            {move || {
                s.on(Feature::ExportAll)
                    .then(|| {
                        view! {
                            <button class="btn" title="Gefilterte Geräteliste als CSV" on:click=move |_| export_filtered(s)>
                                <Icon name="download"/>
                                "CSV"
                            </button>
                        }
                    })
            }}
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
                <div class="dev-line">
                    {
                        let star_id = device.id.clone();
                        move || {
                            let star_id = star_id.clone();
                            s.on(Feature::Favorites)
                                .then(|| {
                                    let class_id = star_id.clone();
                                    view! {
                                        <button
                                            class=move || if s.favorites.with(|f| f.contains(&class_id)) { "star on" } else { "star" }
                                            aria-label="Favorit"
                                            title="Als Favorit anheften"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                s.toggle_favorite(&star_id);
                                            }
                                        >
                                            <Icon name="star"/>
                                        </button>
                                    }
                                })
                        }
                    }
                    <span class="dev-id mono">{device.id.clone()}</span>
                </div>
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
                    {
                        let trend_id = device.id.clone();
                        move || {
                            if !s.on(Feature::Sparklines) {
                                return None;
                            }
                            let path = s
                                .trends
                                .with(|t| t.get(&trend_id).map(|p| insights::sparkline_path(p, 96.0, 18.0)))
                                .filter(|p| !p.is_empty())?;
                            Some(view! {
                                <svg class="spark" viewBox="0 0 96 18" width="96" height="18" role="img" aria-label="Ladezustand der letzten 24 Stunden">
                                    <path d=path fill="none"></path>
                                </svg>
                            })
                        }
                    }
                </div>
            </td>
            <td data-label="Entladestrom und Temperatur" title="Entladestrom: So viel Strom gibt die Batterie gerade ab. Es zählt der Betrag, das Vorzeichen ist je nach Messgerät unterschiedlich.">
                <div class="mono tabular">{device.current_ma.map(format::current).unwrap_or_default()}</div>
                <div class="muted tabular">{device.temperature_c.map_or_else(|| "keine Temperatur".to_owned(), format::temperature)}</div>
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
                                <th>"Entladestrom und Temperatur"</th>
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
fn Distribution() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="panel insight" aria-label="Flottenverteilung">
            <h2>"Flottenverteilung"</h2>
            {move || {
                let buckets = s.devices.with(|d| insights::histogram(d));
                let max = buckets.iter().copied().max().unwrap_or(0).max(1);
                insights::BUCKET_LABELS
                    .iter()
                    .zip(buckets)
                    .map(|(label, count)| {
                        let width = count as f64 / max as f64 * 100.0;
                        view! {
                            <div class="dist-row">
                                <span class="muted">{*label}</span>
                                <span class="bar" aria-hidden="true">
                                    <span class="bar-fill" style:width=format!("{width:.0}%")></span>
                                </span>
                                <span class="tabular">{count}</span>
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </section>
    }
}

#[component]
fn DueNext() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="panel insight" aria-label="Als Nächstes fällig">
            <h2>"Als Nächstes fällig"</h2>
            {move || {
                let list = s.devices.with(|d| insights::due_next(d, 5));
                if list.is_empty() {
                    return view! { <p class="muted">"Noch keine Prognosen."</p> }.into_any();
                }
                list.into_iter()
                    .map(|due| {
                        let id = due.id.clone();
                        view! {
                            <div class="due-row">
                                <span class="status">
                                    <span class=format!("dot {}", format::status_class(due.status))></span>
                                    <button class="link mono" on:click=move |_| s.selected.set(Some(id.clone()))>
                                        {due.id.clone()}
                                    </button>
                                </span>
                                <span class="tabular muted">{format::duration_hours(due.hours)}</span>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}

#[component]
fn ActivityLog() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="panel insight" aria-label="Aktivität">
            <h2>"Aktivität"</h2>
            <p class="muted tabular">{move || format!("{} Messwerte in dieser Sitzung", s.messages.get())}</p>
            {move || {
                let list = s.activity.get();
                if list.is_empty() {
                    return view! { <p class="muted">"Seit dem Öffnen ist nichts passiert."</p> }.into_any();
                }
                let now = s.now_ms.get();
                list.into_iter()
                    .take(6)
                    .map(|entry| {
                        view! {
                            <div class="act-row">
                                <span class="muted tabular">{format::age((now - entry.at_ms) as i64)}</span>
                                <span>{entry.text}</span>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}

#[component]
fn Insights() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        {move || {
            let any = s.on(Feature::Distribution) || s.on(Feature::DueNext) || s.on(Feature::Activity);
            any.then(|| {
                view! {
                    <div class="insights">
                        {s.on(Feature::Distribution).then(|| view! { <Distribution/> })}
                        {s.on(Feature::DueNext).then(|| view! { <DueNext/> })}
                        {s.on(Feature::Activity).then(|| view! { <ActivityLog/> })}
                    </div>
                }
            })
        }}
    }
}

#[component]
pub fn Overview() -> impl IntoView {
    let s = expect_context::<Store>();
    let rows = Memo::new(move |_| {
        let filter = s.filter.get();
        if s.on(Feature::Favorites) {
            let pinned = s.favorites.get();
            s.devices
                .with(|devices| filter::apply_pinned(devices, &filter, &pinned))
        } else {
            s.devices.with(|devices| filter::apply(devices, &filter))
        }
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
        <Insights/>
        <AlertsBox/>
        <FilterBar shown=shown/>
        <DeviceTable rows=rows/>
    }
}
