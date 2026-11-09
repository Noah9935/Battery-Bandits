//! Ein Gerät im Detail: als Seitenpanel in Übersicht und Karte und als eigene Seite unter `/geraete/:id`.
//! Beide zeigen dieselbe Ansicht, nur das Raster ist verschieden.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{HistoryResponse, Range};
use shared::{AlertKind, AlertStatus, TaskKind};
use wasm_bindgen::JsCast;

use crate::features::Feature;
use crate::history::History;
use crate::icons::Icon;
use crate::overview::{Origin, StatusDot};
use crate::state::{ModalKind, Store};
use crate::{api, chart, format, insights, jsutil, route};

/// Was eine offene Meldung bedeutet und was man als Nächstes prüft.
pub fn alert_hint(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Critical => "Die Batterie ist fast leer. Einen Wechsel planen, bevor das Gerät ausfällt.",
        AlertKind::Offline => "Das Gerät sendet nicht mehr. Stromversorgung und Funkverbindung prüfen.",
        AlertKind::Fault => {
            "Das Gerät war gut geladen und ist trotzdem verstummt. Eher Technik oder Funk als die Batterie."
        }
        AlertKind::Stagnation => {
            "Spannung und Strom ändern sich seit Stunden nicht. Verdacht auf einen hängenden Sensor."
        }
        AlertKind::Drop => {
            "Die Spannung ist innerhalb kurzer Zeit stark gefallen. Verdacht auf einen Zellschaden oder eine hohe Last."
        }
    }
}

/// Link auf die eigene Seite eines Geräts. Ein normaler Link, damit "in neuem Tab öffnen" und Kopieren gehen;
/// ein einfacher Klick wechselt ohne Neuladen.
#[component]
pub fn DeviceLink(id: String, #[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    let s = expect_context::<Store>();
    let href = route::device_path(&id).unwrap_or_else(|| "/".to_owned());
    let click_id = id.clone();
    view! {
        <a
            class=format!("link {class}")
            href=href
            on:click=move |ev| {
                if ev.ctrl_key() || ev.meta_key() || ev.shift_key() || ev.button() != 0 {
                    return;
                }
                ev.prevent_default();
                ev.stop_propagation();
                s.open_device_page(&click_id);
            }
        >
            {children()}
        </a>
    }
}

#[component]
fn Cell(label: &'static str, value: String, #[prop(optional, into)] hint: String) -> impl IntoView {
    view! {
        <div class="cell">
            <span class="cell-label">{label}</span>
            <span class="cell-value mono-num">{value}</span>
            {(!hint.is_empty()).then(|| view! { <span class="cell-hint mono-num">{hint}</span> })}
        </div>
    }
}

/// Runde Anzeige des Ladezustands. Die Farbe bleibt neutral, der Status steht als Punkt im Kopf.
fn ring(soc: Option<f32>) -> impl IntoView {
    let (dash, whole) = insights::ring_dash(soc.unwrap_or(0.0), 30.0);
    view! {
        <svg class="ring" viewBox="0 0 72 72" width="56" height="56" role="img" aria-label="Ladezustand">
            <circle class="ring-bg" cx="36" cy="36" r="30" fill="none"></circle>
            <circle
                class="ring-fg"
                cx="36"
                cy="36"
                r="30"
                fill="none"
                stroke-dasharray=format!("{dash:.1} {whole:.1}")
                transform="rotate(-90 36 36)"
            ></circle>
        </svg>
    }
}

#[component]
fn HistoryChart(history: HistoryResponse, critical_pct: f32, warning_pct: f32) -> impl IntoView {
    let range = history.range;
    let model = chart::build(&history.points, history.forecast, critical_pct, range, move |ms| {
        jsutil::axis_label(ms, range)
    });
    if model.is_empty() {
        return view! { <p class="empty">"Für diesen Zeitraum liegen noch keine Messwerte vor."</p> }.into_any();
    }
    let view_box = format!("0 0 {} {}", model.width, model.height);
    let s = expect_context::<Store>();
    let hover = RwSignal::new(None::<usize>);
    let points = model.points.clone();
    let label_points = model.points.clone();
    let cross_points = model.points.clone();
    let (top, bottom) = (model.top, model.bottom);
    let warning_y = chart::y_for(f64::from(warning_pct));
    let on_move = move |ev: leptos::ev::MouseEvent| {
        if !s.on(Feature::Crosshair) {
            return;
        }
        if let Some(element) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let rect = element.get_bounding_client_rect();
            if rect.width() > 0.0 {
                let x = (f64::from(ev.client_x()) - rect.left()) / rect.width() * chart::WIDTH;
                hover.set(chart::nearest(&points, x));
            }
        }
    };
    view! {
        <svg
            class="chart"
            viewBox=view_box
            role="img"
            aria-label="Verlauf des Ladezustands in Prozent"
            on:mousemove=on_move
            on:mouseleave=move |_| hover.set(None)
        >
            {model
                .y_ticks
                .iter()
                .map(|t| {
                    view! {
                        <g>
                            <line class="grid" x1=model.left x2=model.right y1=t.position y2=t.position></line>
                            <text class="axis" x=model.left - 6.0 y=t.position + 4.0 text-anchor="end">{t.label.clone()}</text>
                        </g>
                    }
                })
                .collect_view()}
            {model
                .x_ticks
                .iter()
                .map(|t| {
                    view! {
                        <text class="axis" x=t.position y=model.bottom + 18.0 text-anchor="middle">{t.label.clone()}</text>
                    }
                })
                .collect_view()}
            <line class="threshold warn" x1=model.left x2=model.right y1=warning_y y2=warning_y></line>
            <text class="threshold-label warn" x=model.right y=warning_y - 4.0 text-anchor="end">{format!("Warnung {warning_pct:.0} %")}</text>
            <line class="threshold" x1=model.left x2=model.right y1=model.threshold_y y2=model.threshold_y></line>
            <text class="threshold-label" x=model.right y=model.threshold_y - 4.0 text-anchor="end">{format!("kritisch {critical_pct:.0} %")}</text>
            <path class="soc-line" d=model.soc_path.clone() fill="none"></path>
            {model.forecast_path.clone().map(|d| view! { <path class="forecast-line" d=d fill="none"></path> })}
            {move || {
                hover
                    .get()
                    .and_then(|i| cross_points.get(i).cloned())
                    .map(|p| {
                        view! {
                            <line class="cross" x1=p.x x2=p.x y1=top y2=bottom></line>
                            <circle class="cross-dot" cx=p.x cy=p.y r="3"></circle>
                        }
                    })
            }}
        </svg>
        <p class="cross-label mono-num">
            {move || {
                hover
                    .get()
                    .and_then(|i| label_points.get(i).cloned())
                    .and_then(|p| {
                        chrono::DateTime::from_timestamp_millis(p.time_ms)
                            .map(|t| format!("{}: {}", jsutil::date_time_label(t), format::percent(p.soc)))
                    })
                    .unwrap_or_default()
            }}
        </p>
        <p class="legend">
            "Durchgezogen: Ladezustand. Gestrichelt: Prognose bei gleichbleibender Entladung. Waagerecht: Warn- und kritischer Wert."
        </p>
    }
    .into_any()
}

/// Gemeinsame Ansicht. `full` ist die eigene Seite, sonst das schmale Seitenpanel.
#[component]
pub fn DeviceView(id: String, full: bool) -> impl IntoView {
    let s = expect_context::<Store>();
    let range = RwSignal::new(Range::Day);
    let history = RwSignal::new(None::<HistoryResponse>);
    let history_error = RwSignal::new(None::<String>);

    let load_id = id.clone();
    Effect::new(move |_| {
        let range = range.get();
        s.history_tick.track();
        let id = load_id.clone();
        spawn_local(async move {
            match api::history(&id, range).await {
                Ok(h) => {
                    history.set(Some(h));
                    history_error.set(None);
                }
                Err(e) => history_error.set(Some(e)),
            }
        });
    });

    let device = {
        let id = id.clone();
        Memo::new(move |_| s.device(&id))
    };
    let critical_pct = move || {
        s.settings
            .with(|st| st.as_ref().map_or(20.0, |st| st.threshold_critical_pct))
    };
    let warning_pct = move || {
        s.settings
            .with(|st| st.as_ref().map_or(40.0, |st| st.threshold_warning_pct))
    };
    let planned = {
        let id = id.clone();
        move || s.planned_task(&id)
    };
    let alerts = {
        let id = id.clone();
        move || {
            s.alerts.with(|list| {
                list.iter()
                    .filter(|a| a.device_id == id && a.status == AlertStatus::Open)
                    .cloned()
                    .collect::<Vec<_>>()
            })
        }
    };
    let plan = {
        let id = id.clone();
        move |_| {
            s.modal.set(Some(ModalKind::Plan {
                device_id: id.clone(),
                alert_id: None,
            }))
        }
    };

    let export_id = id.clone();
    let page_id = id.clone();
    view! {
        {move || match device.get() {
            None => {
                view! {
                    <div class="empty-state">
                        <p class="empty">
                            {if s.loaded.get() { "Dieses Gerät gibt es nicht (mehr)." } else { "Daten werden geladen." }}
                        </p>
                        {(full && s.loaded.get()).then(|| view! { <button class="btn" on:click=move |_| s.go_page(crate::state::Page::Overview)>"Zur Übersicht"</button> })}
                    </div>
                }
                    .into_any()
            }
            Some(d) => {
                let seen = d.last_seen_at;
                let alerts = alerts.clone();
                let export_id = export_id.clone();
                let page_id = page_id.clone();
                let forecast_text = match (d.discharge_rate_pct_per_hour, d.forecast_hours_remaining, d.soc_percent) {
                    (Some(rate), Some(hours), Some(soc)) => {
                        let to_critical = ((soc - critical_pct()) / rate).max(0.0);
                        format!(
                            "Bei gleichbleibender Entladung ({} Prozentpunkte pro Stunde) ist die Batterie in {} leer und in {} unter dem kritischen Wert.",
                            format::number(f64::from(rate), 2),
                            format::duration_hours(hours),
                            format::duration_hours(to_critical),
                        )
                    }
                    _ => "Noch keine Prognose: Es fehlen Messwerte oder das Gerät wird gerade nicht entladen.".to_owned(),
                };
                let short_forecast = insights::forecast_short(&d, critical_pct()).unwrap_or_else(|| "keine Prognose".to_owned());
                let route_href = match (d.lat, d.lon) {
                    (Some(lat), Some(lon)) => Some(format!("https://www.google.com/maps/dir/?api=1&destination={lat:.6},{lon:.6}")),
                    _ => None,
                };
                let title = d.name.clone().unwrap_or_default();
                view! {
                    <article class=if full { "device device-page" } else { "device device-panel" }>
                        <header class="device-head">
                            <div class="device-title">
                                {full
                                    .then(|| {
                                        view! {
                                            <button class="btn btn-ghost" on:click=move |_| s.go_page(crate::state::Page::Overview)>
                                                "Zurück zur Übersicht"
                                            </button>
                                        }
                                    })}
                                <h1 class="device-id mono-num">{d.id.clone()}</h1>
                                {(!title.is_empty()).then(|| view! { <span class="device-name">{title.clone()}</span> })}
                                <StatusDot status=d.status/>
                                <Origin source=d.data_source/>
                            </div>
                            <div class="device-actions">
                                {route_href
                                    .map(|href| {
                                        view! {
                                            <a class="btn" href=href target="_blank" rel="noopener">
                                                <Icon name="map-pin"/>
                                                "Route in Google Maps"
                                            </a>
                                        }
                                    })}
                                <a class="btn" href=move || api::export_url(&export_id, range.get()) download="">
                                    <Icon name="download"/>
                                    "CSV"
                                </a>
                                {(!full)
                                    .then(|| {
                                        view! {
                                            <button class="btn" on:click=move |_| s.open_device_page(&page_id)>"Eigene Seite öffnen"</button>
                                        }
                                    })}
                            </div>
                        </header>
                        {move || {
                            alerts()
                                .into_iter()
                                .map(|a| {
                                    let tone = match a.kind {
                                        AlertKind::Critical => "diag crit",
                                        AlertKind::Offline | AlertKind::Fault => "diag off",
                                        AlertKind::Stagnation | AlertKind::Drop => "diag warn",
                                    };
                                    view! {
                                        <div class=tone role="alert">
                                            <strong>{a.kind.label()}</strong>
                                            <span>{alert_hint(a.kind)}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }}
                        <section class="tele" aria-label="Telemetrie">
                            <div class="cell cell-soc">
                                <span class="cell-label">"Ladestand"</span>
                                <span class="cell-soc-row">
                                    {s.on(Feature::Ring).then(|| ring(d.soc_percent))}
                                    <span class="cell-value cell-big mono-num">
                                        {d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)}
                                    </span>
                                </span>
                                <span class="cell-hint mono-num">{short_forecast}</span>
                            </div>
                            <Cell label="Spannung" value=d.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage)/>
                            <Cell label="Entladestrom" value=d.current_ma.map_or_else(|| "unbekannt".to_owned(), format::current)/>
                            <Cell label="Temperatur" value=d.temperature_c.map_or_else(|| "wird nicht gesendet".to_owned(), format::temperature)/>
                            <Cell
                                label="Position"
                                value=match (d.lat, d.lon) {
                                    (Some(lat), Some(lon)) => format!("{}, {}", format::coordinate(lat), format::coordinate(lon)),
                                    _ => "Kein GPS-Fix".to_owned(),
                                }
                                hint=d.satellites.map(|n| format!("{n} Satelliten")).unwrap_or_default()
                            />
                            <Cell
                                label="Letzter Kontakt"
                                value=seen.map_or_else(|| "noch keiner".to_owned(), jsutil::date_time_label)
                                hint=seen.map(|t| format::age((s.now_ms.get_untracked() - t.timestamp_millis() as f64) as i64)).unwrap_or_default()
                            />
                            <Cell
                                label="Batterie seit"
                                value=d.battery_installed_at.map_or_else(|| "unbekannt".to_owned(), jsutil::date_label)
                                hint=format!("{} Ah entnommen", format::number(d.discharged_ah, 1))
                            />
                        </section>
                        <div class="device-grid">
                            <section class="surface device-chart">
                                <div class="surface-head">
                                    <h2>"Ladezustand im Verlauf"</h2>
                                    <div class="seg" role="group" aria-label="Zeitraum">
                                        {Range::ALL
                                            .into_iter()
                                            .map(|r| {
                                                view! {
                                                    <button
                                                        class=move || if range.get() == r { "seg-btn is-active" } else { "seg-btn" }
                                                        aria-pressed=move || (range.get() == r).to_string()
                                                        on:click=move |_| range.set(r)
                                                    >
                                                        {r.label()}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                                <div class="surface-body">
                                    {move || {
                                        if let Some(e) = history_error.get() {
                                            return view! { <p class="empty">{e}</p> }.into_any();
                                        }
                                        match history.get() {
                                            Some(h) => {
                                                view! { <HistoryChart history=h critical_pct=critical_pct() warning_pct=warning_pct()/> }
                                                    .into_any()
                                            }
                                            None => view! { <p class="empty">"Verlauf wird geladen."</p> }.into_any(),
                                        }
                                    }}
                                    <p class="forecast-text">{forecast_text}</p>
                                </div>
                            </section>
                            <aside class="device-side">
                                <section class="surface">
                                    <div class="surface-head"><h2>"Einsatz"</h2></div>
                                    <div class="surface-body">
                                        {
                                            let plan = plan.clone();
                                            let planned = planned.clone();
                                            let device_id = d.id.clone();
                                            move || match planned() {
                                                Some(task) => {
                                                    let task_id = task.id;
                                                    let device_id = device_id.clone();
                                                    let battery = task.kind == TaskKind::Battery;
                                                    view! {
                                                        <p>
                                                            {format!(
                                                                "{} geplant seit {}. {}.",
                                                                task.kind.label(),
                                                                jsutil::date_label(task.created_at),
                                                                crate::tickets::assignee_text(&task),
                                                            )}
                                                        </p>
                                                        <div class="btn-row">
                                                            <button
                                                                class="btn"
                                                                on:click=move |_| s.modal.set(Some(ModalKind::Ticket { task_id }))
                                                            >
                                                                "Einsatz öffnen"
                                                            </button>
                                                            {battery
                                                                .then(|| {
                                                                    view! {
                                                                        <button
                                                                            class="btn btn-primary"
                                                                            on:click=move |_| {
                                                                                s.modal
                                                                                    .set(
                                                                                        Some(ModalKind::Complete {
                                                                                            task_id,
                                                                                            device_id: device_id.clone(),
                                                                                        }),
                                                                                    )
                                                                            }
                                                                        >
                                                                            "Batterie gewechselt"
                                                                        </button>
                                                                    }
                                                                })}
                                                        </div>
                                                    }
                                                        .into_any()
                                                }
                                                None => {
                                                    view! {
                                                        <p class="muted">"Noch kein Einsatz geplant."</p>
                                                        <div class="btn-row">
                                                            <button class="btn btn-primary" on:click=plan.clone()>"Einsatz planen"</button>
                                                        </div>
                                                    }
                                                        .into_any()
                                                }
                                            }
                                        }
                                    </div>
                                </section>
                                <section class="surface">
                                    <div class="surface-head"><h2>"Protokoll"</h2></div>
                                    <div class="surface-body">
                                        <History device=d.id.clone() limit=if full { 30 } else { 8 }/>
                                    </div>
                                </section>
                            </aside>
                        </div>
                    </article>
                }
                    .into_any()
            }
        }}
    }
}

/// Eigene Seite eines Geräts (`/geraete/:id`).
#[component]
pub fn DevicePage(id: String) -> impl IntoView {
    view! {
        <div class="view view-device">
            <DeviceView id=id full=true/>
        </div>
    }
}

/// Seitenpanel über Übersicht oder Karte.
#[component]
pub fn Drawer() -> impl IntoView {
    let s = expect_context::<Store>();
    let close = move || s.selected.set(None);
    view! {
        {move || {
            s.selected
                .get()
                .map(|id| {
                    view! {
                        <div class="scrim" on:click=move |_| close()></div>
                        <aside
                            class="drawer"
                            role="dialog"
                            aria-modal="true"
                            aria-label="Gerätedetails"
                            on:keydown=move |ev| {
                                if ev.key() == "Escape" {
                                    close();
                                }
                            }
                        >
                            <button class="icon-btn drawer-close" aria-label="Schließen" on:click=move |_| close()>
                                <Icon name="x"/>
                            </button>
                            <DeviceView id=id full=false/>
                        </aside>
                    }
                })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_alert_kind_has_a_hint_without_dashes() {
        for kind in AlertKind::ALL {
            let text = alert_hint(*kind);
            assert!(text.ends_with('.'), "{kind:?}");
            assert!(!text.contains(['\u{2013}', '\u{2014}']), "{kind:?}");
        }
    }

    #[test]
    fn hints_differ_per_kind() {
        let mut texts: Vec<&str> = AlertKind::ALL.iter().map(|k| alert_hint(*k)).collect();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), AlertKind::ALL.len());
    }
}
