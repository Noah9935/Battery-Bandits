//! Übersicht. Drei Fragen in dieser Reihenfolge: Was ist los (ein Satz), was kommt als Nächstes (eine Karte),
//! und dann alles andere (Flottenleiste, Meldungen, Geräteliste).
//!
//! Zeilen und Zellen bleiben im Dokument stehen und ändern bei neuen Messwerten nur ihre Werte. So flackert
//! nichts, Ringe und Säulen gleiten weich auf den neuen Stand, und ein Mauszeiger verliert nie sein Ziel.

use leptos::prelude::*;
use shared::api::{DeviceSort, SortDir};
use shared::{Alert, AlertKind, AlertStatus, DataSource, Device, DeviceStatus};

use crate::device::DeviceLink;
use crate::features::Feature;
use crate::icons::Icon;
use crate::overlays::export_filtered;
use crate::state::{ModalKind, Store};
use crate::{filter, format, insights};

/// Status als Pille: farbiger Punkt mit Text, nie nur Farbe.
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

// ---------------------------------------------------------------- Hero

/// Der Satz oben auf der Übersicht. Die Zahl steht getrennt, damit sie groß gesetzt und gezählt werden kann.
#[derive(Debug, Clone, PartialEq)]
pub struct HeroText {
    pub number: Option<i64>,
    pub text: String,
    pub sub: String,
}

/// Sagt in einem Blick, ob etwas zu tun ist.
pub fn hero_text(open_alerts: i64, total: i64, critical: i64, warning: i64, offline: i64) -> HeroText {
    if total == 0 {
        return HeroText {
            number: None,
            text: "Noch keine Geräte.".to_owned(),
            sub: "Sobald TrafficNodes Messwerte senden, erscheinen sie hier.".to_owned(),
        };
    }
    if open_alerts == 0 {
        return HeroText {
            number: None,
            text: "Alles in Ordnung.".to_owned(),
            sub: if warning > 0 {
                format!("{warning} im Warnbereich, alle anderen sind in Ordnung.")
            } else {
                format!("Alle {total} Geräte senden und sind ausreichend geladen.")
            },
        };
    }
    let mut parts = Vec::new();
    if critical > 0 {
        parts.push(format!("{critical} kritisch"));
    }
    if offline > 0 {
        parts.push(format!("{offline} offline"));
    }
    if warning > 0 {
        parts.push(format!("{warning} mit Warnung"));
    }
    let sub = if parts.is_empty() {
        "Es liegen offene Meldungen vor.".to_owned()
    } else {
        format!("{} von {total} Geräten.", parts.join(", "))
    };
    if open_alerts == 1 {
        HeroText {
            number: None,
            text: "Ein Gerät braucht dich.".to_owned(),
            sub,
        }
    } else {
        HeroText {
            number: Some(open_alerts),
            text: "Geräte brauchen dich.".to_owned(),
            sub,
        }
    }
}

#[component]
fn Hero() -> impl IntoView {
    let s = expect_context::<Store>();
    let hero = Memo::new(move |_| {
        let m = s.summary.get();
        hero_text(m.open_alerts, m.total, m.critical, m.warning, m.offline)
    });
    view! {
        <div class="hero-main">
            <p class="eyebrow">
                <span class=move || if s.live.get() { "live-dot is-on" } else { "live-dot" }></span>
                {move || if s.live.get() { "Flotte, live" } else { "Flotte, letzter Stand" }}
            </p>
            <h1 class="hero-title">
                // Die Zahl zählt per CSS hoch (siehe .count im Stylesheet), der Text steht für Vorleseprogramme daneben.
                <span
                    class=move || if hero.with(|h| h.number.is_some()) { "count" } else { "count is-hidden" }
                    style=move || format!("--n: {}", hero.with(|h| h.number.unwrap_or(0)))
                    aria-hidden="true"
                ></span>
                <span class="sr-only">{move || hero.with(|h| h.number.map(|n| n.to_string()).unwrap_or_default())}</span>
                <span class="hero-words">{move || hero.with(|h| h.text.clone())}</span>
            </h1>
            <p class="lead">{move || hero.with(|h| h.sub.clone())}</p>
        </div>
    }
}

// ---------------------------------------------------------------- Als Nächstes

/// Das eine Gerät, um das man sich als Erstes kümmern sollte.
#[derive(Debug, Clone, PartialEq)]
pub struct NextUp {
    pub id: String,
    pub status: DeviceStatus,
    /// Die große Angabe: Restlaufzeit, Ladestand oder "Offline".
    pub big: String,
    pub small: String,
}

/// Zuerst zählt die kürzeste Restlaufzeit. Gibt es keine Prognose, die dringendste offene Meldung.
pub fn next_up(devices: &[Device], open_alerts: Vec<Alert>) -> Option<NextUp> {
    if let Some(due) = insights::due_next(devices, 1).into_iter().next() {
        return Some(NextUp {
            id: due.id,
            status: due.status,
            big: format::duration_hours(due.hours),
            small: "bis die Batterie leer ist".to_owned(),
        });
    }
    let alert = prioritized(open_alerts, devices).into_iter().next()?;
    let device = devices.iter().find(|d| d.id == alert.device_id);
    let silent = matches!(alert.kind, AlertKind::Offline | AlertKind::Fault);
    Some(NextUp {
        status: device.map_or(DeviceStatus::Offline, |d| d.status),
        big: match device.and_then(|d| d.soc_percent) {
            Some(soc) if !silent => format::percent(soc),
            _ => "Offline".to_owned(),
        },
        small: alert.kind.label().to_owned(),
        id: alert.device_id,
    })
}

#[component]
fn Spotlight() -> impl IntoView {
    let s = expect_context::<Store>();
    let next = Memo::new(move |_| s.devices.with(|d| next_up(d, s.open_alerts())));
    let planned = Memo::new(move |_| next.with(|n| n.as_ref().is_some_and(|n| s.planned_task(&n.id).is_some())));
    view! {
        <aside class="spotlight" aria-label="Als Nächstes fällig">
            <span class="eyebrow">"Als Nächstes"</span>
            {move || match next.get() {
                None => {
                    view! {
                        <p class="spot-big">"Nichts fällig."</p>
                        <p class="muted">"Keine Batterie wird in absehbarer Zeit leer."</p>
                    }
                        .into_any()
                }
                Some(n) => {
                    let plan_id = n.id.clone();
                    let open_id = n.id.clone();
                    view! {
                        <div class="spot-top">
                            <DeviceLink id=n.id.clone() class="spot-id">{n.id.clone()}</DeviceLink>
                            <StatusDot status=n.status/>
                        </div>
                        <p class="spot-big mono-num">{n.big.clone()}</p>
                        <p class="muted">{n.small.clone()}</p>
                        <div class="btn-row">
                            {if planned.get() {
                                view! { <span class="planned"><Icon name="check"/>"Einsatz geplant"</span> }.into_any()
                            } else {
                                view! {
                                    <button
                                        class="btn btn-primary"
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
                            <button class="btn btn-ghost" on:click=move |_| s.selected.set(Some(open_id.clone()))>
                                "Ansehen"
                            </button>
                        </div>
                    }
                        .into_any()
                }
            }}
        </aside>
    }
}

// ---------------------------------------------------------------- Flottenleiste

/// Kurzer Hinweistext einer Säule: Kennung, Ladestand und Status.
pub fn fleet_tip(device: &Device) -> String {
    format!(
        "{}: {}, {}",
        device.id,
        device
            .soc_percent
            .map_or_else(|| "Ladestand unbekannt".to_owned(), format::percent),
        format::status_label(device.status)
    )
}

#[component]
fn FleetCell(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let dev = {
        let id = id.clone();
        Memo::new(move |_| s.device(&id))
    };
    let status = Memo::new(move |_| dev.with(|d| d.as_ref().map_or(DeviceStatus::Offline, |d| d.status)));
    let fill = Memo::new(move |_| {
        dev.with(|d| d.as_ref().and_then(|d| d.soc_percent))
            .map_or(0, |v| v.clamp(0.0, 100.0).round() as i32)
    });
    let open_id = id.clone();
    view! {
        <button
            class=move || format!("fleet-cell st-{}", format::status_class(status.get()))
            title=move || dev.with(|d| d.as_ref().map(fleet_tip).unwrap_or_default())
            on:click=move |_| s.selected.set(Some(open_id.clone()))
        >
            <span class="fleet-bar">
                <span class="fleet-fill" style:height=move || format!("{}%", fill.get())></span>
            </span>
            <span class="fleet-id mono-num">{id}</span>
        </button>
    }
}

/// Die ganze Flotte als Reihe kleiner Säulen: Füllhöhe ist der Ladestand, die Farbe der Status.
/// Die Reihenfolge folgt der Kennung und bleibt stehen, damit man ein Gerät immer an derselben Stelle findet.
#[component]
fn FleetStrip() -> impl IntoView {
    let s = expect_context::<Store>();
    let ids = Memo::new(move |_| s.devices.with(|d| d.iter().map(|d| d.id.clone()).collect::<Vec<_>>()));
    view! {
        <section class="fleet" aria-label="Die Flotte auf einen Blick">
            <div class="fleet-head">
                <span class="eyebrow">"Die Flotte auf einen Blick"</span>
                <span class="fleet-legend">
                    {DeviceStatus::ALL
                        .iter()
                        .map(|st| {
                            view! {
                                <span class="legend-item">
                                    <span class=format!("dot {}", format::status_class(*st))></span>
                                    {format::status_label(*st)}
                                </span>
                            }
                        })
                        .collect_view()}
                </span>
            </div>
            <div class="fleet-strip">
                <For each=move || ids.get() key=|id| id.clone() children=move |id| view! { <FleetCell id=id/> }/>
            </div>
        </section>
    }
}

// ---------------------------------------------------------------- Meldungen

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
    // Ein Memo, damit die Knöpfe nicht bei jedem Nachladen neu gezeichnet werden.
    let planned = {
        let device_id = device_id.clone();
        Memo::new(move |_| s.planned_task(&device_id).is_some())
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
        <li class=format!("todo-row {}", alert_dot(kind))>
            <div class="todo-top">
                <DeviceLink id=device_id.clone() class="todo-id">{device_id.clone()}</DeviceLink>
                <span class="status">
                    <span class=format!("dot {}", alert_dot(kind))></span>
                    {format::alert_kind_label(kind)}
                </span>
            </div>
            <p class="todo-detail mono-num">{detail}</p>
            <div class="todo-actions">
                {move || {
                    if planned.get() {
                        view! { <span class="planned"><Icon name="check"/>"Einsatz geplant"</span> }.into_any()
                    } else {
                        view! { <button class="btn btn-sm btn-primary" on:click=plan.clone()>{plan_label(kind)}</button> }.into_any()
                    }
                }}
                <button class="btn btn-sm btn-ghost" on:click=dismiss>"Abweisen"</button>
            </div>
        </li>
    }
}

#[component]
fn AlertsBox() -> impl IntoView {
    let s = expect_context::<Store>();
    let sorted = move || s.devices.with(|d| prioritized(s.open_alerts(), d));
    let count = Memo::new(move |_| {
        s.alerts
            .with(|a| a.iter().filter(|a| a.status == AlertStatus::Open).count())
    });
    view! {
        <section class="todo" aria-label="Offene Meldungen">
            <div class="section-head">
                <h2>"Zu tun"</h2>
                <span class="count-pill mono-num">{move || count.get()}</span>
            </div>
            {move || {
                if count.get() == 0 {
                    return view! { <p class="calm">"Keine offenen Meldungen. Schön ruhig hier."</p> }.into_any();
                }
                view! {
                    <ul class="todo-list">
                        <For each=sorted key=|a| (a.id, a.status) children=move |alert| view! { <AlertRow alert=alert/> }/>
                    </ul>
                }
                    .into_any()
            }}
        </section>
    }
}

// ---------------------------------------------------------------- Filter

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

/// Die fünf Status-Filter mit ihrer Beschriftung, in der Reihenfolge der Dringlichkeit.
pub const STATUS_FILTERS: [(Option<DeviceStatus>, &str); 5] = [
    (None, "Alle"),
    (Some(DeviceStatus::Critical), "Kritisch"),
    (Some(DeviceStatus::Warning), "Warnung"),
    (Some(DeviceStatus::Offline), "Offline"),
    (Some(DeviceStatus::Normal), "In Ordnung"),
];

#[component]
fn FilterBar(shown: Signal<usize>) -> impl IntoView {
    let s = expect_context::<Store>();
    let count_of = move |status: Option<DeviceStatus>| {
        let m = s.summary.get();
        match status {
            None => m.total,
            Some(DeviceStatus::Critical) => m.critical,
            Some(DeviceStatus::Warning) => m.warning,
            Some(DeviceStatus::Offline) => m.offline,
            Some(DeviceStatus::Normal) => m.normal,
        }
    };
    view! {
        <div class="toolbar" role="search" aria-label="Filter">
            <div class="chips" role="group" aria-label="Status">
                {STATUS_FILTERS
                    .into_iter()
                    .map(|(status, label)| {
                        let active = move || s.filter.with(|f| f.status == status);
                        view! {
                            <button
                                class=move || if active() { "chip is-active" } else { "chip" }
                                aria-pressed=move || active().to_string()
                                on:click=move |_| s.filter.update(|f| f.status = status)
                            >
                                {status.map(|st| view! { <span class=format!("dot {}", format::status_class(st))></span> })}
                                {label}
                                <span class="chip-n mono-num">{move || count_of(status)}</span>
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <div class="toolbar-tools">
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
                <div class="seg" role="group" aria-label="Herkunft der Daten">
                    <button
                        class=move || if s.filter.with(|f| f.source.is_none()) { "seg-btn is-active" } else { "seg-btn" }
                        on:click=move |_| s.filter.update(|f| f.source = None)
                    >
                        "Alle"
                    </button>
                    {DataSource::ALL
                        .iter()
                        .map(|src| {
                            let src = *src;
                            view! {
                                <button
                                    class=move || if s.filter.with(|f| f.source == Some(src)) { "seg-btn is-active" } else { "seg-btn" }
                                    on:click=move |_| s.filter.update(|f| f.source = Some(src))
                                >
                                    {format::source_label(src)}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
                <label class="sort">
                    <span class="sr-only">"Sortieren nach"</span>
                    <select
                        aria-label="Sortieren nach"
                        on:change=move |ev| s.filter.update(|f| f.sort = parse_sort(&event_target_value(&ev)))
                    >
                        {[
                            (DeviceSort::Id, "Nach Kennung"),
                            (DeviceSort::SocPercent, "Nach Ladestand"),
                            (DeviceSort::LastSeenAt, "Nach letzter Meldung"),
                            (DeviceSort::Status, "Nach Status"),
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
                <label class="check" title="Kritische und ausgefallene Geräte stehen oben">
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
                                <button
                                    class="btn btn-icon"
                                    aria-label="Gefilterte Geräteliste als CSV speichern"
                                    title="Gefilterte Geräteliste als CSV"
                                    on:click=move |_| export_filtered(s)
                                >
                                    <Icon name="download"/>
                                </button>
                            }
                        })
                }}
            </div>
            <span
                class="count-text mono-num"
                title=move || {
                    let (real, invented) = s.devices.with(|d| filter::count_by_source(d));
                    format::source_summary(real, invented)
                }
            >
                {move || format!("{} von {} Geräten", shown.get(), s.summary.get().total)}
            </span>
        </div>
    }
}

// ---------------------------------------------------------------- Geräteliste

#[component]
fn DeviceRow(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let dev = {
        let id = id.clone();
        Memo::new(move |_| s.device(&id))
    };
    let status = Memo::new(move |_| dev.with(|d| d.as_ref().map_or(DeviceStatus::Offline, |d| d.status)));
    let source = Memo::new(move |_| dev.with(|d| d.as_ref().map_or(DataSource::Simulation, |d| d.data_source)));
    let soc = Memo::new(move |_| dev.with(|d| d.as_ref().and_then(|d| d.soc_percent)));
    // Liest einen Wert des Geräts als Text. Die Zelle bleibt stehen, nur ihr Text ändert sich.
    let text = move |pick: fn(&Device) -> String| move || dev.with(|d| d.as_ref().map(pick).unwrap_or_default());
    let forecast = move || {
        let critical = s
            .settings
            .with(|st| st.as_ref().map_or(20.0, |st| st.threshold_critical_pct));
        dev.with(|d| d.as_ref().and_then(|d| insights::forecast_short(d, critical)))
            .unwrap_or_else(|| "keine Prognose".to_owned())
    };
    let seen = move || match dev.with(|d| d.as_ref().and_then(|d| d.last_seen_at)) {
        Some(t) => format::age((s.now_ms.get() - t.timestamp_millis() as f64) as i64),
        None => "noch keine".to_owned(),
    };
    let open = {
        let id = id.clone();
        move || s.selected.set(Some(id.clone()))
    };
    let open_click = open.clone();
    let star_id = id.clone();
    let trend_id = id.clone();
    let link_id = id.clone();
    view! {
        <tr
            class=move || format!("row st-{}", format::status_class(status.get()))
            tabindex="0"
            on:click=move |_| open_click()
            on:keydown=move |ev| {
                if ev.key() == "Enter" || ev.key() == " " {
                    ev.prevent_default();
                    open();
                }
            }
        >
            <td class="c-dev" data-label="Gerät">
                <div class="dev-cell">
                    <div class="dev-line">
                        {move || {
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
                        }}
                        <DeviceLink id=link_id class="dev-id mono-num">{id.clone()}</DeviceLink>
                    </div>
                    <div class="dev-meta">
                        {move || view! { <Origin source=source.get()/> }}
                        <span class="dev-name">{text(|d| d.name.clone().unwrap_or_default())}</span>
                    </div>
                </div>
            </td>
            <td class="c-soc" data-label="Ladestand">
                <div class="soc">
                    {move || {
                        if s.on(Feature::Ring) {
                            view! {
                                <svg class="soc-ring" viewBox="0 0 28 28" width="30" height="30" aria-hidden="true">
                                    <circle class="ring-bg" cx="14" cy="14" r="11" fill="none"></circle>
                                    <circle
                                        class="ring-fg"
                                        cx="14"
                                        cy="14"
                                        r="11"
                                        fill="none"
                                        stroke-dasharray=move || {
                                            let (dash, whole) = insights::ring_dash(soc.get().unwrap_or(0.0), 11.0);
                                            format!("{dash:.1} {whole:.1}")
                                        }
                                        transform="rotate(-90 14 14)"
                                    ></circle>
                                </svg>
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="bar" aria-hidden="true">
                                    <div
                                        class="bar-fill"
                                        style:width=move || format!("{:.0}%", soc.get().unwrap_or(0.0).clamp(0.0, 100.0))
                                    ></div>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                    <strong class="mono-num">{move || soc.get().map_or_else(|| "unbekannt".to_owned(), format::percent)}</strong>
                </div>
            </td>
            <td class="c-status" data-label="Status">{move || view! { <StatusDot status=status.get()/> }}</td>
            <td
                class="c-volt"
                data-label="Spannung"
                title="Unter der Spannung steht der Entladestrom. Temperatur und Position stehen im Detail."
            >
                <div class="stack">
                    <span class="mono-num stack-main">
                        {text(|d| d.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage))}
                    </span>
                    <span class="mono-num stack-sub">{text(|d| d.current_ma.map(format::amps).unwrap_or_default())}</span>
                </div>
            </td>
            <td class="c-fc mono-num" data-label="Prognose">{forecast}</td>
            <td class="c-trend" data-label="Trend">
                {move || {
                    if !s.on(Feature::Sparklines) {
                        return None;
                    }
                    let path = s
                        .trends
                        .with(|t| t.get(&trend_id).map(|p| insights::sparkline_path(p, 96.0, 22.0)))
                        .filter(|p| !p.is_empty())?;
                    Some(view! {
                        <svg class="spark" viewBox="0 0 96 22" width="96" height="22" role="img" aria-label="Ladezustand der letzten 24 Stunden">
                            <path d=path fill="none"></path>
                        </svg>
                    })
                }}
            </td>
            <td class="c-seen mono-num" data-label="Zuletzt">{seen}</td>
            <td class="c-go" aria-hidden="true"><Icon name="chevron-right"/></td>
        </tr>
    }
}

#[component]
fn DeviceTable(rows: Memo<Vec<Device>>) -> impl IntoView {
    let s = expect_context::<Store>();
    // Nur die Reihenfolge der Kennungen: Ändert sich ein Messwert, bleibt jede Zeile, wo sie ist.
    let ids = Memo::new(move |_| rows.with(|r| r.iter().map(|d| d.id.clone()).collect::<Vec<_>>()));
    let empty = Memo::new(move |_| ids.with(Vec::is_empty));
    view! {
        {move || {
            if empty.get() {
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
                            <th>"Gerät"</th>
                            <th>"Ladestand"</th>
                            <th>"Status"</th>
                            <th>"Spannung und Strom"</th>
                            <th class="num">"Prognose"</th>
                            <th class="c-trend">"Trend"</th>
                            <th class="num">"Zuletzt"</th>
                            <th class="c-go"></th>
                        </tr>
                    </thead>
                    <tbody>
                        <For each=move || ids.get() key=|id| id.clone() children=move |id| view! { <DeviceRow id=id/> }/>
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
        <section class="session-log" aria-label="Aktivität in dieser Sitzung">
            <div class="section-head">
                <h2>"Gerade eben"</h2>
                <span class="count-pill mono-num">{move || format!("{} Messwerte", s.messages.get())}</span>
            </div>
            {move || {
                let list = s.activity.get();
                if list.is_empty() {
                    return view! { <p class="calm">"Seit dem Öffnen ist nichts passiert."</p> }.into_any();
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
    view! {
        <div class="view view-home">
            <section class="hero">
                <Hero/>
                {move || s.on(Feature::DueNext).then(|| view! { <Spotlight/> })}
            </section>
            {move || s.on(Feature::Distribution).then(|| view! { <FleetStrip/> })}
            <div class="workspace">
                <aside class="col-todo">
                    <AlertsBox/>
                    {move || s.on(Feature::Activity).then(|| view! { <ActivityLog/> })}
                </aside>
                <section class="col-list" aria-label="Geräte">
                    <div class="section-head">
                        <h2>"Geräte"</h2>
                    </div>
                    <FilterBar shown=shown/>
                    <DeviceTable rows=rows/>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

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
    fn the_hero_says_whether_something_needs_doing() {
        assert_eq!(hero_text(0, 0, 0, 0, 0).text, "Noch keine Geräte.");
        assert_eq!(hero_text(0, 15, 0, 0, 0).text, "Alles in Ordnung.");
        assert_eq!(
            hero_text(0, 15, 0, 3, 0).sub,
            "3 im Warnbereich, alle anderen sind in Ordnung."
        );
        let one = hero_text(1, 15, 1, 0, 0);
        assert_eq!((one.number, one.text.as_str()), (None, "Ein Gerät braucht dich."));
        let many = hero_text(14, 15, 11, 0, 3);
        assert_eq!((many.number, many.text.as_str()), (Some(14), "Geräte brauchen dich."));
        assert_eq!(many.sub, "11 kritisch, 3 offline von 15 Geräten.");
        assert_eq!(hero_text(2, 15, 0, 0, 0).sub, "Es liegen offene Meldungen vor.");
    }

    #[test]
    fn the_shortest_forecast_is_next_up() {
        let soon = Device {
            forecast_hours_remaining: Some(1.5),
            ..device("SOON", Some(12.0))
        };
        let later = Device {
            forecast_hours_remaining: Some(30.0),
            ..device("LATER", Some(60.0))
        };
        let next = next_up(&[later, soon], vec![alert(1, "LATER", AlertKind::Critical)]).unwrap();
        assert_eq!(next.id, "SOON");
        assert_eq!(next.big, format::duration_hours(1.5));
        assert_eq!(next.small, "bis die Batterie leer ist");
    }

    #[test]
    fn without_a_forecast_the_most_urgent_alert_is_next_up() {
        let devices = [device("EMPTY", Some(0.0)), device("GONE", Some(80.0))];
        let alerts = vec![
            alert(1, "GONE", AlertKind::Offline),
            alert(2, "EMPTY", AlertKind::Critical),
        ];
        let next = next_up(&devices, alerts).unwrap();
        assert_eq!((next.id.as_str(), next.small.as_str()), ("EMPTY", "Ladestand kritisch"));
        assert_eq!(next.big, format::percent(0.0));

        let silent = next_up(&devices, vec![alert(1, "GONE", AlertKind::Offline)]).unwrap();
        assert_eq!(silent.big, "Offline", "bei Funkstille zählt nicht der alte Ladestand");
        assert_eq!(next_up(&devices, Vec::new()), None);
    }

    #[test]
    fn the_fleet_tip_names_charge_and_status() {
        assert_eq!(fleet_tip(&device("SIM03", Some(12.5))), "SIM03: 12,5 %, Kritisch");
        assert_eq!(
            fleet_tip(&device("SIM07", None)),
            "SIM07: Ladestand unbekannt, Kritisch"
        );
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

    #[test]
    fn the_status_filters_cover_every_status_once() {
        for status in DeviceStatus::ALL {
            assert_eq!(
                STATUS_FILTERS.iter().filter(|(s, _)| *s == Some(*status)).count(),
                1,
                "{status:?}"
            );
        }
        assert_eq!(STATUS_FILTERS[0], (None, "Alle"));
    }
}
