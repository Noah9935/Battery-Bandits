//! Übersicht: Telemetrie-Leiste, priorisierte Meldungen und die Gerätetabelle als zweigeteilter Arbeitsbereich.

use leptos::prelude::*;
use shared::api::{DeviceSort, SortDir};
use shared::{Alert, AlertKind, DataSource, Device, DeviceStatus};

use crate::device::DeviceLink;
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
            class=move || if active() { "stat is-active" } else { "stat" }
            aria-pressed=move || active().to_string()
            on:click=move |_| {
                s.filter.update(|f| f.status = if f.status == status { None } else { status });
            }
        >
            <span class="stat-label">
                {status.map(|st| view! { <span class=format!("dot {}", format::status_class(st))></span> })}
                {label}
            </span>
            <span class="stat-count mono-num">{move || count.get()}</span>
        </button>
    }
}

/// Verteilung der Flotte auf die fünf Ladebereiche als eine durchgehende Leiste.
#[component]
fn FleetBar() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <div class="tele-block" aria-label="Flottenverteilung">
            <span class="tele-title">"Flottenverteilung"</span>
            {move || {
                let buckets = s.devices.with(|d| insights::histogram(d));
                let total: usize = buckets.iter().sum();
                view! {
                    <div class="fleetbar" role="img" aria-label="Verteilung der Geräte nach Ladestand">
                        {buckets
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| **n > 0)
                            .map(|(i, n)| {
                                let width = *n as f64 / total.max(1) as f64 * 100.0;
                                view! {
                                    <span
                                        class=format!("fleetbar-seg b{i}")
                                        style:width=format!("{width:.2}%")
                                        title=format!("{}: {n}", insights::BUCKET_LABELS[i])
                                    ></span>
                                }
                            })
                            .collect_view()}
                    </div>
                    <div class="fleetbar-legend">
                        {insights::BUCKET_LABELS
                            .iter()
                            .zip(buckets)
                            .enumerate()
                            .map(|(i, (label, n))| {
                                view! {
                                    <span class="fleetbar-key">
                                        <span class=format!("swatch b{i}")></span>
                                        {*label}
                                        " "
                                        <b class="mono-num">{n}</b>
                                    </span>
                                }
                            })
                            .collect_view()}
                    </div>
                }
            }}
        </div>
    }
}

/// Das Gerät mit der kürzesten Restlaufzeit samt direktem Knopf zum Planen.
#[component]
fn DueNext() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <div class="tele-block tele-due" aria-label="Als Nächstes fällig">
            <span class="tele-title">"Als Nächstes fällig"</span>
            {move || {
                let list = s.devices.with(|d| insights::due_next(d, 3));
                let Some(first) = list.first().cloned() else {
                    return view! { <p class="muted">"Noch keine Prognosen."</p> }.into_any();
                };
                let planned = s.planned_task(&first.id).is_some();
                let plan_id = first.id.clone();
                view! {
                    <div class="due-main">
                        <span class="status">
                            <span class=format!("dot {}", format::status_class(first.status))></span>
                            <DeviceLink id=first.id.clone() class="mono-num">{first.id.clone()}</DeviceLink>
                        </span>
                        <span class="due-hours mono-num">{format::duration_hours(first.hours)}</span>
                        {if planned {
                            view! { <span class="muted">"Einsatz geplant"</span> }.into_any()
                        } else {
                            view! {
                                <button
                                    class="btn btn-primary btn-sm"
                                    on:click=move |_| {
                                        s.modal
                                            .set(
                                                Some(ModalKind::Plan {
                                                    device_id: plan_id.clone(),
                                                    alert_id: None,
                                                }),
                                            )
                                    }
                                >
                                    "Einsatz planen"
                                </button>
                            }
                                .into_any()
                        }}
                    </div>
                    <div class="due-more">
                        {list
                            .into_iter()
                            .skip(1)
                            .map(|due| {
                                view! {
                                    <span class="due-next">
                                        <DeviceLink id=due.id.clone() class="mono-num">{due.id.clone()}</DeviceLink>
                                        <span class="mono-num muted">{format::duration_hours(due.hours)}</span>
                                    </span>
                                }
                            })
                            .collect_view()}
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}

#[component]
fn Telemetry() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="telemetry" aria-label="Kennzahlen">
            <div class="stats">
                <Stat label="Gesamtflotte" status=None count=Signal::derive(move || s.summary.get().total)/>
                <Stat label="Kritisch" status=Some(DeviceStatus::Critical) count=Signal::derive(move || s.summary.get().critical)/>
                <Stat label="Warnung" status=Some(DeviceStatus::Warning) count=Signal::derive(move || s.summary.get().warning)/>
                <Stat label="Offline" status=Some(DeviceStatus::Offline) count=Signal::derive(move || s.summary.get().offline)/>
            </div>
            {move || s.on(Feature::Distribution).then(|| view! { <FleetBar/> })}
            {move || s.on(Feature::DueNext).then(|| view! { <DueNext/> })}
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

/// Reihenfolge der Meldungen: leere Batterien zuerst, dann Auffälligkeiten, zuletzt Funkstille.
pub fn alert_rank(kind: AlertKind) -> u8 {
    match kind {
        AlertKind::Critical => 0,
        AlertKind::Drop => 1,
        AlertKind::Fault => 2,
        AlertKind::Stagnation => 3,
        AlertKind::Offline => 4,
    }
}

/// Meldungen nach Dringlichkeit, bei gleicher Art der niedrigere Ladestand zuerst, dann die Kennung.
pub fn prioritized(mut alerts: Vec<Alert>, devices: &[Device]) -> Vec<Alert> {
    let soc = |a: &Alert| {
        devices
            .iter()
            .find(|d| d.id == a.device_id)
            .and_then(|d| d.soc_percent)
            .unwrap_or(f32::MAX)
    };
    alerts.sort_by(|a, b| {
        alert_rank(a.kind)
            .cmp(&alert_rank(b.kind))
            .then_with(|| soc(a).total_cmp(&soc(b)))
            .then_with(|| a.device_id.cmp(&b.device_id))
    });
    alerts
}

#[component]
fn AlertRow(alert: Alert) -> impl IntoView {
    let s = expect_context::<Store>();
    let device_id = alert.device_id.clone();
    let alert_id = alert.id;
    let kind = alert.kind;
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
        <li class=format!("alert-row {}", alert_dot(kind))>
            <div class="alert-top">
                <span class="status">
                    <span class=format!("dot {}", alert_dot(kind))></span>
                    {format::alert_kind_label(kind)}
                </span>
                <DeviceLink id=device_id.clone() class="mono-num">{device_id.clone()}</DeviceLink>
            </div>
            <div class="alert-detail mono-num">{detail}</div>
            <div class="alert-actions">
                {move || {
                    if planned() {
                        view! { <span class="muted">"Einsatz geplant"</span> }.into_any()
                    } else {
                        view! { <button class="btn btn-primary btn-sm" on:click=plan.clone()>{plan_label(kind)}</button> }.into_any()
                    }
                }}
                <button class="btn btn-sm" on:click=dismiss>"Abweisen"</button>
                <button class="btn btn-ghost btn-sm" on:click=open>"Öffnen"</button>
            </div>
        </li>
    }
}

#[component]
fn AlertsBox() -> impl IntoView {
    let s = expect_context::<Store>();
    let sorted = move || s.devices.with(|d| prioritized(s.open_alerts(), d));
    // Ein Memo meldet nur, ob es Meldungen gibt, nicht jede Änderung der Liste: Die Zeilen werden nicht bei jedem Messwert neu gezeichnet.
    let has_alerts = Memo::new(move |_| {
        s.alerts
            .with(|a| a.iter().any(|a| a.status == shared::AlertStatus::Open))
    });
    view! {
        <section class="surface alerts" aria-label="Offene Meldungen">
            <div class="surface-head">
                <h2>"Offene Meldungen"</h2>
                <span class="count mono-num">{move || sorted().len()}</span>
            </div>
            {move || {
                if !has_alerts.get() {
                    return view! { <p class="calm">"Keine offenen Meldungen."</p> }.into_any();
                }
                view! {
                    <ul class="alert-list">
                        <For each=sorted key=|a| (a.id, a.status) children=move |alert| view! { <AlertRow alert=alert/> }/>
                    </ul>
                }
                    .into_any()
            }}
        </section>
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
fn FilterBar(shown: Signal<usize>, origin_text: Signal<String>) -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <div class="toolbar" role="search" aria-label="Filter">
            <label class="search">
                <Icon name="search"/>
                <input
                    id="search-input"
                    type="search"
                    placeholder="Gerät suchen (Taste /)"
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
                class="btn btn-icon"
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
            <span class="count-text mono-num" title=move || origin_text.get()>
                {move || format!("{} von {} Geräten", shown.get(), s.summary.get().total)}
            </span>
        </div>
    }
}

fn row_key(d: &Device) -> String {
    format!(
        "{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        d.id,
        d.status,
        d.soc_percent,
        d.last_seen_at,
        d.name,
        d.lat,
        d.data_source,
        d.voltage_mv,
        d.forecast_hours_remaining
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
    let forecast = {
        let device = device.clone();
        move || {
            let critical = s
                .settings
                .with(|st| st.as_ref().map_or(20.0, |st| st.threshold_critical_pct));
            insights::forecast_short(&device, critical).unwrap_or_else(|| "keine Prognose".to_owned())
        }
    };
    let link_id = device.id.clone();
    let link_text = device.id.clone();
    view! {
        <tr
            class=format!("row st-{}", format::status_class(device.status))
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
                <div class="dev-cell">
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
                    <DeviceLink id=link_id class="dev-id mono-num">{link_text}</DeviceLink>
                </div>
                <div class="dev-meta">
                    <Origin source=device.data_source/>
                    {device.name.clone().filter(|n| !n.is_empty()).map(|n| view! { <span class="dev-name">{n}</span> })}
                </div>
                </div>
            </td>
            <td data-label="Ladestand">
                <div class="soc">
                    <div class="bar" aria-hidden="true">
                        <div class="bar-fill" style:width=format!("{width:.0}%")></div>
                    </div>
                    {
                        let ring_value = soc;
                        move || {
                            s.on(Feature::Ring)
                                .then(|| {
                                    let (dash, whole) = insights::ring_dash(ring_value.unwrap_or(0.0), 7.0);
                                    view! {
                                        <svg class="mini-ring" viewBox="0 0 18 18" width="18" height="18" aria-hidden="true">
                                            <circle class="ring-bg" cx="9" cy="9" r="7" fill="none"></circle>
                                            <circle
                                                class="ring-fg"
                                                cx="9"
                                                cy="9"
                                                r="7"
                                                fill="none"
                                                stroke-dasharray=format!("{dash:.1} {whole:.1}")
                                                transform="rotate(-90 9 9)"
                                            ></circle>
                                        </svg>
                                    }
                                })
                        }
                    }
                    <strong class="mono-num">{soc.map_or_else(|| "unbekannt".to_owned(), format::percent)}</strong>
                </div>
            </td>
            <td data-label="Spannung" class="mono-num">{device.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage)}</td>
            <td
                data-label="Strom"
                class="mono-num"
                title="Entladestrom: So viel Strom gibt die Batterie gerade ab. Es zählt der Betrag, das Vorzeichen ist je nach Messgerät unterschiedlich."
            >
                {device.current_ma.map_or_else(|| "unbekannt".to_owned(), format::current)}
            </td>
            <td data-label="Temperatur" class="mono-num col-temp">
                {device.temperature_c.map_or_else(|| "keine".to_owned(), format::temperature)}
            </td>
            <td data-label="Prognose" class="mono-num">{forecast}</td>
            <td data-label="Trend" class="col-trend">
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
            </td>
            <td data-label="Letzte Meldung" class="mono-num">
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
                <table class="devices">
                    <thead>
                        <tr>
                            <th>"Status"</th>
                            <th>"Gerät"</th>
                            <th>"Ladestand"</th>
                            <th class="num">"Spannung"</th>
                            <th class="num">"Strom"</th>
                            <th class="num col-temp">"Temperatur"</th>
                            <th class="num">"Prognose"</th>
                            <th class="col-trend">"Trend"</th>
                            <th class="num">"Letzte Meldung"</th>
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
            }
            .into_any()
        }}
    }
}

#[component]
fn ActivityLog() -> impl IntoView {
    let s = expect_context::<Store>();
    view! {
        <section class="surface" aria-label="Aktivität in dieser Sitzung">
            <div class="surface-head">
                <h2>"Aktivität"</h2>
                <span class="count mono-num">{move || format!("{} Messwerte", s.messages.get())}</span>
            </div>
            <div class="surface-body">
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
                                    <span class="muted mono-num">{format::age((now - entry.at_ms) as i64)}</span>
                                    <span>{entry.text}</span>
                                </div>
                            }
                        })
                        .collect_view()
                        .into_any()
                }}
            </div>
        </section>
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
        <div class="view view-cockpit">
            <h1 class="sr-only">"Übersicht"</h1>
            <Telemetry/>
            <div class="workspace">
                <aside class="col col-alerts" aria-label="Meldungen">
                    <AlertsBox/>
                    {move || s.on(Feature::Activity).then(|| view! { <ActivityLog/> })}
                </aside>
                <section class="col col-table" aria-label="Geräte">
                    <FilterBar shown=shown origin_text=origin_text/>
                    <div class="table-scroll">
                        <DeviceTable rows=rows/>
                    </div>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use shared::AlertStatus;

    fn alert(id: i64, device: &str, kind: AlertKind) -> Alert {
        Alert {
            id,
            device_id: device.into(),
            kind,
            status: AlertStatus::Open,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            closed_at: None,
        }
    }

    fn device(id: &str, soc: Option<f32>) -> Device {
        Device {
            id: id.into(),
            name: None,
            status: DeviceStatus::Critical,
            soc_percent: soc,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn critical_alerts_come_first_then_anomalies_then_silence() {
        let alerts = vec![
            alert(1, "A", AlertKind::Offline),
            alert(2, "B", AlertKind::Stagnation),
            alert(3, "C", AlertKind::Critical),
            alert(4, "D", AlertKind::Drop),
            alert(5, "E", AlertKind::Fault),
        ];
        let order: Vec<String> = prioritized(alerts, &[]).into_iter().map(|a| a.device_id).collect();
        assert_eq!(order, ["C", "D", "E", "B", "A"]);
    }

    #[test]
    fn within_one_kind_the_emptier_device_comes_first() {
        let alerts = vec![
            alert(1, "FULL", AlertKind::Critical),
            alert(2, "EMPTY", AlertKind::Critical),
            alert(3, "UNKNOWN", AlertKind::Critical),
        ];
        let devices = [
            device("FULL", Some(19.0)),
            device("EMPTY", Some(2.0)),
            device("UNKNOWN", None),
        ];
        let ids: Vec<String> = prioritized(alerts, &devices).into_iter().map(|a| a.device_id).collect();
        assert_eq!(ids, ["EMPTY", "FULL", "UNKNOWN"]);
    }

    #[test]
    fn every_kind_has_a_distinct_rank() {
        let mut ranks: Vec<u8> = AlertKind::ALL.iter().map(|k| alert_rank(*k)).collect();
        ranks.sort_unstable();
        ranks.dedup();
        assert_eq!(ranks.len(), AlertKind::ALL.len());
    }

    #[test]
    fn plan_labels_distinguish_batteries_from_checks() {
        assert_eq!(plan_label(AlertKind::Critical), "Batteriewechsel planen");
        assert_eq!(plan_label(AlertKind::Stagnation), "Kontrolle planen");
    }
}
