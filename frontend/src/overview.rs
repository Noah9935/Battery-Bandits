//! Übersicht. Oben die Dringlichkeits-Anzeige (welche Geräte brauchen jetzt jemanden, und seit wann), darunter die
//! Merkliste, die Meldungen und die Geräteliste.
//!
//! Zeilen und Zellen bleiben im Dokument stehen und ändern bei neuen Messwerten nur ihre Werte. So flackert
//! nichts, Ringe und Säulen gleiten weich auf den neuen Stand, und ein Mauszeiger verliert nie sein Ziel.

use gloo_timers::callback::Interval;
use leptos::html;
use leptos::prelude::*;
use shared::api::{DeviceSort, SortDir};
use shared::{Alert, AlertKind, AlertStatus, DataSource, Device, DeviceStatus};

use crate::device::DeviceLink;
use crate::features::Feature;
use crate::icons::Icon;
use crate::overlays::export_filtered;
use crate::pager::Pager;
use crate::pins::{PinButton, PinList};
use crate::state::{ModalKind, Store};
use crate::swap::SwapButton;
use crate::urgency::{self, Thresholds, Urgent};
use crate::{filter, format, gesture, insights, jsutil};

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

/// Kennzeichnung erfundener Geräte (Simulator). Echte Geräte bekommen nichts, damit nur das Besondere auffällt.
#[component]
pub fn SimBadge(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let simulated = Memo::new(move |_| {
        s.devices
            .with(|d| d.iter().any(|d| d.id == id && d.data_source == DataSource::Simulation))
    });
    view! { <Show when=move || simulated.get()><Origin source=DataSource::Simulation/></Show> }
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

// ---------------------------------------------------------------- Dringlichkeits-Anzeige

/// Eine Karte der Anzeige. Sie liest ihre Werte nach Kennung aus der Liste, damit Ladestand und Dauer
/// weiterlaufen, ohne dass die Karte neu aufgebaut wird.
#[component]
fn TickerCard(id: String, items: Memo<Vec<Urgent>>) -> impl IntoView {
    let s = expect_context::<Store>();
    let item = {
        let id = id.clone();
        Memo::new(move |_| items.with(|list| list.iter().find(|u| u.device_id == id).cloned()))
    };
    let view_id = id.clone();
    let swap_id = id.clone();
    let pin_id = id.clone();
    let badge_id = id.clone();
    let link_id = id.clone();
    view! {
        <article class=move || {
            format!("tk-card {}", item.with(|i| i.as_ref().map_or("offline", |i| i.tone.class())))
        }>
            <div class="tk-top">
                <span class="tk-tone">
                    <span class=move || format!("dot {}", item.with(|i| i.as_ref().map_or("offline", |i| i.tone.class())))></span>
                    {move || item.with(|i| i.as_ref().map(|i| i.tone.word()).unwrap_or_default())}
                </span>
                <SimBadge id=badge_id/>
                <PinButton id=pin_id/>
            </div>
            <DeviceLink id=link_id class="tk-id mono-num">
                {move || {
                    item.with(|i| {
                        i.as_ref()
                            .map(|i| format::device_title(&i.device_id, i.name.as_deref()))
                            .unwrap_or_default()
                    })
                }}
            </DeviceLink>
            <p class="tk-since mono-num">
                {move || item.with(|i| i.as_ref().map(|i| urgency::state_line(i, s.now_ms.get())).unwrap_or_default())}
            </p>
            <p class="muted tk-detail">{move || item.with(|i| i.as_ref().map(|i| i.detail.clone()).unwrap_or_default())}</p>
            <div class="tk-actions">
                <SwapButton device_id=swap_id/>
                <button class="btn" on:click=move |_| s.selected.set(Some(view_id.clone()))>"Ansehen"</button>
            </div>
        </article>
    }
}

/// Die dringenden Geräte als waagerechte Leiste: Sie läuft von selbst durch, lässt sich aber auch mit dem Finger
/// durchwischen. Jede Karte zeigt, wie lange das Gerät schon in diesem Zustand ist.
#[component]
fn Ticker() -> impl IntoView {
    let s = expect_context::<Store>();
    let items = Memo::new(move |_| {
        let t = s.settings.with(|st| {
            st.as_ref().map_or(Thresholds::default(), |st| Thresholds {
                critical_pct: st.threshold_critical_pct,
                warning_pct: st.threshold_warning_pct,
            })
        });
        s.devices.with(|d| s.alerts.with(|a| urgency::urgent(d, a, t)))
    });
    let headline = Memo::new(move |_| items.with(|i| urgency::headline(i, s.devices.with(Vec::len))));
    let ids = Memo::new(move |_| items.with(|i| i.iter().map(|u| u.device_id.clone()).collect::<Vec<_>>()));
    // Der Punkt vor der Überschrift zeigt den schlimmsten Zustand der Flotte und leuchtet nur grün, wenn alles in Ordnung ist.
    let worst = Memo::new(move |_| items.with(|i| i.first().map(|u| u.tone)));
    let track = NodeRef::<html::Div>::new();
    let index = RwSignal::new(0usize);
    let paused = StoredValue::new(false);

    // Alle zehn Sekunden zur nächsten Karte, solange niemand die Leiste berührt. Die Karten gleiten langsam.
    Interval::new(gesture::TICKER_INTERVAL_MS, move || {
        if paused.get_value() || jsutil::reduced_motion() {
            return;
        }
        if let Some(el) = track.get_untracked()
            && let Some(next) = gesture::next_card_offset(
                f64::from(el.scroll_left()),
                f64::from(el.client_width()),
                f64::from(el.scroll_width()),
            )
        {
            jsutil::slide_to(&el, next, gesture::TICKER_SLIDE_MS);
        }
    })
    .forget();

    let on_scroll = move |_| {
        if let Some(el) = track.get_untracked() {
            let now = gesture::card_index(f64::from(el.scroll_left()), f64::from(el.client_width()));
            if index.get_untracked() != now {
                index.set(now);
            }
        }
    };
    let go = move |i: usize| {
        if let Some(el) = track.get_untracked() {
            jsutil::slide_to(&el, f64::from(el.client_width()) * i as f64, gesture::ARROW_SLIDE_MS);
        }
    };
    // Die Pfeile blättern eine Seite weiter oder zurück. Am Ende geht es zum Anfang und umgekehrt.
    let step = move |forward: bool| {
        if let Some(el) = track.get_untracked() {
            let (left, width, total) = (
                f64::from(el.scroll_left()),
                f64::from(el.client_width()),
                f64::from(el.scroll_width()),
            );
            let target = if forward {
                gesture::next_card_offset(left, width, total)
            } else {
                gesture::prev_card_offset(left, width, total)
            };
            if let Some(target) = target {
                jsutil::slide_to(&el, target, gesture::ARROW_SLIDE_MS);
            }
        }
    };
    let several = Memo::new(move |_| ids.with(Vec::len) > 1);
    view! {
        <section class="ticker" aria-label="Dringende Geräte">
            <p class="eyebrow">
                <span class=move || urgency::headline_dot_class(worst.get(), s.live.get())></span>
                {move || headline.get()}
            </p>
            <div class="tk-stage" on:mouseenter=move |_| paused.set_value(true) on:mouseleave=move |_| paused.set_value(false)>
            <Show when=move || several.get()>
                <button class="tk-arrow tk-prev" aria-label="Vorherige Karten" on:click=move |_| step(false)>
                    <Icon name="chevron-left"/>
                </button>
                <button class="tk-arrow tk-next" aria-label="Nächste Karten" on:click=move |_| step(true)>
                    <Icon name="chevron-right"/>
                </button>
            </Show>
            <div
                class="tk-track"
                data-noswipe=""
                node_ref=track
                on:scroll=on_scroll
                on:touchstart=move |_| {
                    paused.set_value(true);
                    if let Some(el) = track.get_untracked() {
                        jsutil::slide_cancel(&el);
                    }
                }
                on:touchend=move |_| paused.set_value(false)
            >
                {move || {
                    if ids.with(Vec::is_empty) {
                        let text = if s.devices.with(Vec::is_empty) {
                            "Noch keine Geräte. Sobald TrafficNodes senden, erscheinen sie hier."
                        } else {
                            "Nichts zu tun. Alle Geräte senden und sind ausreichend geladen."
                        };
                        return view! { <article class="tk-card calm-card"><p class="tk-id">{text}</p></article> }.into_any();
                    }
                    view! {
                        <For
                            each=move || ids.get()
                            key=|id| id.clone()
                            children=move |id| view! { <TickerCard id=id items=items/> }
                        />
                    }
                        .into_any()
                }}
            </div>
            </div>
            <div class="tk-dots" role="group" aria-label="Karte wählen">
                {move || {
                    (0..ids.with(Vec::len))
                        .map(|i| {
                            view! {
                                <button
                                    class=move || if index.get() == i { "tk-dot is-active" } else { "tk-dot" }
                                    aria-label=format!("Karte {}", i + 1)
                                    on:click=move |_| go(i)
                                ></button>
                            }
                        })
                        .collect_view()
                }}
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
    // Am Handy zunächst zugeklappt, damit man nicht erst durch die ganze Liste muss. Der Zähler bleibt sichtbar.
    let open = RwSignal::new(!jsutil::is_narrow());
    let count = Memo::new(move |_| {
        s.alerts
            .with(|a| a.iter().filter(|a| a.status == AlertStatus::Open).count())
    });
    view! {
        <section class="todo" aria-label="Offene Meldungen">
            <button
                class="fold-head"
                aria-expanded=move || open.get().to_string()
                aria-label=move || if open.get() { "Zu tun zuklappen" } else { "Zu tun aufklappen" }
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <h2>"Zu tun"</h2>
                <span class="fold-end">
                    <span class="count-pill mono-num">{move || count.get()}</span>
                    {move || view! { <Icon name=if open.get() { "chevron-up" } else { "chevron-down" }/> }}
                </span>
            </button>
            {move || {
                if !open.get() {
                    return ().into_any();
                }
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

/// Ändert den Filter, ohne die Seite springen zu lassen: Die Liste ändert sich an Ort und Stelle, die Leiste mit den
/// Filtern bleibt, wo sie ist.
fn change(s: Store, update: impl FnOnce(&mut crate::filter::ViewFilter) + 'static) {
    jsutil::keep_in_place("device-toolbar", "device-area", move || s.filter.update(update));
}

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
        <div id="device-toolbar" class="toolbar" role="search" aria-label="Filter">
            <div class="chips" role="group" aria-label="Status">
                {STATUS_FILTERS
                    .into_iter()
                    .map(|(status, label)| {
                        let active = move || s.filter.with(|f| f.status == status);
                        view! {
                            <button
                                class=move || if active() { "chip is-active" } else { "chip" }
                                aria-pressed=move || active().to_string()
                                on:click=move |_| change(s, move |f| f.status = status)
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
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            change(s, move |f| f.search = value);
                        }
                    />
                </label>
                <div class="seg" role="group" aria-label="Herkunft der Daten">
                    <button
                        class=move || if s.filter.with(|f| f.source.is_none()) { "seg-btn is-active" } else { "seg-btn" }
                        on:click=move |_| change(s, |f| f.source = None)
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
                                    on:click=move |_| change(s, move |f| f.source = Some(src))
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
                        on:change=move |ev| {
                            let sort = parse_sort(&event_target_value(&ev));
                            change(s, move |f| f.sort = sort);
                        }
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
                        change(s, |f| f.dir = if f.dir == SortDir::Asc { SortDir::Desc } else { SortDir::Asc });
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
                        on:change=move |ev| {
                            let on = event_target_checked(&ev);
                            change(s, move |f| f.attention_first = on);
                        }
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
    let swap_id = id.clone();
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
                        <PinButton id=star_id.clone()/>
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
            <td class="c-swap">
                {move || {
                    status
                        .get()
                        .needs_attention()
                        .then(|| view! { <SwapButton device_id=swap_id.clone() compact=true/> })
                }}
            </td>
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
                            <th class="c-swap"></th>
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
    // Höchstens 20 Geräte je Seite. Ein anderer Filter beginnt wieder auf der ersten Seite.
    let page = RwSignal::new(0usize);
    Effect::new(move |_| {
        s.filter.track();
        page.set(0);
    });
    let page_rows = Memo::new(move |_| {
        let (start, end, _) = filter::page_bounds(rows.with(Vec::len), page.get(), filter::PAGE_SIZE);
        rows.with(|r| r[start..end].to_vec())
    });
    view! {
        <div class="view view-home">
            <Ticker/>
            <div class="workspace">
                <aside class="col-todo">
                    <PinList/>
                    <AlertsBox/>
                    {move || s.on(Feature::Activity).then(|| view! { <ActivityLog/> })}
                </aside>
                <section class="col-list" aria-label="Geräte">
                    <div class="section-head">
                        <h2>"Geräte"</h2>
                    </div>
                    <FilterBar shown=shown/>
                    <div id="device-area">
                        <Pager
                            total=shown
                            page=page
                            size=filter::PAGE_SIZE
                            label="Seiten der Geräteliste (oben)"
                            anchor="device-toolbar"
                            top=true
                        />
                        <DeviceTable rows=page_rows/>
                        <Pager
                            total=shown
                            page=page
                            size=filter::PAGE_SIZE
                            label="Seiten der Geräteliste"
                            anchor="device-toolbar"
                        />
                    </div>
                </section>
            </div>
            <crate::retire::RetiredList/>
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
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
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
