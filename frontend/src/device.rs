//! Ein Gerät im Detail: als Blatt, das von rechts hereingleitet, und als eigene Seite unter `/geraete/:id`.
//! Beide zeigen dieselbe Ansicht, nur das Raster ist verschieden.
//!
//! Wie in der Übersicht bleibt das Gerüst stehen und nur die Werte ändern sich. Das Diagramm wird bei neuen
//! Messwerten nicht neu aufgebaut, das Fadenkreuz bleibt also unter dem Finger.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{HistoryResponse, Range};
use shared::{AlertKind, AlertStatus, DataSource, Device, DeviceStatus, MaintenanceTask, TaskKind};
use wasm_bindgen::JsCast;

use crate::efficiency_view::EfficiencySection;
use crate::features::Feature;
use crate::forward::DeviceShare;
use crate::history::History;
use crate::icons::Icon;
use crate::nodes::SubscribeButton;
use crate::overview::{Origin, StatusDot};
use crate::pins::PinButton;
use crate::state::{ModalKind, Page, Store};
use crate::swap::SwapButton;
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

/// Ein Satz zum Zustand des Geräts, direkt unter der Kennung.
pub fn status_sentence(device: &Device, critical_pct: f32) -> String {
    let forecast = insights::forecast_short(device, critical_pct);
    match (device.status, forecast) {
        (DeviceStatus::Offline, _) => "Offline. Es kommen keine Messwerte mehr an.".to_owned(),
        (DeviceStatus::Critical, Some(f)) => format!("Kritisch. Noch {f}."),
        (DeviceStatus::Critical, None) => "Kritisch. Die Batterie ist fast leer.".to_owned(),
        (DeviceStatus::Warning, Some(f)) => format!("Warnung. Noch {f}."),
        (DeviceStatus::Warning, None) => "Warnung. Der Ladestand ist niedrig.".to_owned(),
        (DeviceStatus::Normal, Some(f)) => format!("In Ordnung. Noch {f}."),
        (DeviceStatus::Normal, None) => "In Ordnung.".to_owned(),
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

/// Verlauf des Ladezustands. Das Bild bleibt stehen, nur Pfade und Beschriftung ändern sich.
#[component]
fn HistoryChart(
    history: RwSignal<Option<HistoryResponse>>,
    error: RwSignal<Option<String>>,
    critical: Signal<f32>,
    warning: Signal<f32>,
) -> impl IntoView {
    let s = expect_context::<Store>();
    let model = Memo::new(move |_| {
        history.get().map(|h| {
            let range = h.range;
            chart::build(&h.points, h.forecast, critical.get(), range, move |ms| {
                jsutil::axis_label(ms, range)
            })
        })
    });
    let has_points = Memo::new(move |_| model.with(|m| m.as_ref().is_some_and(|m| !m.is_empty())));
    let hover = RwSignal::new(None::<usize>);
    let pick = move |f: fn(&chart::ChartModel) -> String| move || model.with(|m| m.as_ref().map(f).unwrap_or_default());
    let on_move = move |ev: leptos::ev::PointerEvent| {
        if !s.on(Feature::Crosshair) {
            return;
        }
        if let Some(element) = ev.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let rect = element.get_bounding_client_rect();
            if rect.width() > 0.0 {
                let x = (f64::from(ev.client_x()) - rect.left()) / rect.width() * chart::WIDTH;
                hover.set(model.with_untracked(|m| m.as_ref().and_then(|m| chart::nearest(&m.points, x))));
            }
        }
    };
    let point = move || {
        hover
            .get()
            .and_then(|i| model.with(|m| m.as_ref().and_then(|m| m.points.get(i).cloned())))
    };
    let warning_y = move || chart::y_for(f64::from(warning.get()));
    let critical_y = move || chart::y_for(f64::from(critical.get()));
    let left = chart::WIDTH - chart::PLOT_RIGHT;
    view! {
        {move || error.get().map(|e| view! { <p class="empty">{e}</p> })}
        <Show
            when=move || has_points.get()
            fallback=move || {
                view! {
                    <p class="empty">
                        {move || {
                            if model.with(Option::is_none) {
                                "Verlauf wird geladen."
                            } else {
                                "Für diesen Zeitraum liegen noch keine Messwerte vor."
                            }
                        }}
                    </p>
                }
            }
        >
            <div class="chart-wrap">
                <svg
                    class="chart"
                    viewBox=format!("0 0 {} {}", chart::WIDTH, chart::HEIGHT)
                    role="img"
                    aria-label="Verlauf des Ladezustands in Prozent"
                    on:pointermove=on_move
                    on:pointerdown=on_move
                    on:pointerleave=move |_| hover.set(None)
                >
                    <defs>
                        <linearGradient id="soc-fill" x1="0" y1="0" x2="0" y2="1">
                            <stop class="area-stop top" offset="0%"></stop>
                            <stop class="area-stop bottom" offset="100%"></stop>
                        </linearGradient>
                    </defs>
                    {move || {
                        model
                            .with(|m| {
                                m.as_ref()
                                    .map(|m| {
                                        let (l, r, b) = (m.left, m.right, m.bottom);
                                        let y = m
                                            .y_ticks
                                            .iter()
                                            .map(|t| {
                                                view! {
                                                    <g>
                                                        <line class="grid" x1=l x2=r y1=t.position y2=t.position></line>
                                                        <text class="axis" x=l - 8.0 y=t.position + 3.5 text-anchor="end">{t.label.clone()}</text>
                                                    </g>
                                                }
                                            })
                                            .collect_view();
                                        let x = m
                                            .x_ticks
                                            .iter()
                                            .map(|t| {
                                                view! { <text class="axis" x=t.position y=b + 18.0 text-anchor="middle">{t.label.clone()}</text> }
                                            })
                                            .collect_view();
                                        (y, x)
                                    })
                            })
                    }}
                    <line class="threshold warn" x1=chart::PLOT_LEFT x2=left y1=warning_y y2=warning_y></line>
                    <text class="threshold-label warn" x=left y=move || warning_y() - 5.0 text-anchor="end">
                        {move || format!("Warnung {:.0} %", warning.get())}
                    </text>
                    <line class="threshold" x1=chart::PLOT_LEFT x2=left y1=critical_y y2=critical_y></line>
                    <text class="threshold-label" x=left y=move || critical_y() - 5.0 text-anchor="end">
                        {move || format!("kritisch {:.0} %", critical.get())}
                    </text>
                    <path class="soc-area" d=pick(|m| m.area_path.clone()) fill="url(#soc-fill)" stroke="none"></path>
                    <path class="soc-line" d=pick(|m| m.soc_path.clone()) fill="none"></path>
                    <path class="forecast-line" d=pick(|m| m.forecast_path.clone().unwrap_or_default()) fill="none"></path>
                    {move || {
                        point()
                            .map(|p| {
                                view! {
                                    <line class="cross" x1=p.x x2=p.x y1=chart::PLOT_TOP y2=chart::HEIGHT - chart::PLOT_BOTTOM></line>
                                    <circle class="cross-dot" cx=p.x cy=p.y r="4.5"></circle>
                                }
                            })
                    }}
                </svg>
                {move || {
                    point()
                        .and_then(|p| {
                            let time = chrono::DateTime::from_timestamp_millis(p.time_ms)?;
                            let side = if p.x > chart::WIDTH * 0.7 { "chart-tip is-left" } else { "chart-tip" };
                            Some(
                                view! {
                                    <div class=side style:left=format!("{:.2}%", p.x / chart::WIDTH * 100.0)>
                                        <strong class="mono-num">{format::percent(p.soc)}</strong>
                                        <span class="mono-num">{jsutil::date_time_label(time)}</span>
                                    </div>
                                },
                            )
                        })
                }}
            </div>
            <p class="legend">
                "Linie: Ladezustand. Gestrichelt: Prognose bei gleichbleibender Entladung. Waagerecht: Warn- und kritischer Wert."
            </p>
        </Show>
    }
}

/// Eine Angabe ohne Kasten: kleine Beschriftung, großer Wert, optional ein Zusatz.
#[component]
pub fn Figure(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(optional, into)] hint: Option<Signal<String>>,
) -> impl IntoView {
    view! {
        <div class="figure">
            <span class="figure-label">{label}</span>
            <span class="figure-value mono-num">{move || value.get()}</span>
            {hint.map(|hint| view! { <span class="figure-hint mono-num">{move || hint.get()}</span> })}
        </div>
    }
}

#[component]
fn DeviceBody(
    id: String,
    full: bool,
    device: Memo<Option<Device>>,
    range: RwSignal<Range>,
    history: RwSignal<Option<HistoryResponse>>,
    history_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let s = expect_context::<Store>();
    let critical = Signal::derive(move || {
        s.settings
            .with(|st| st.as_ref().map_or(20.0, |st| st.threshold_critical_pct))
    });
    let warning = Signal::derive(move || {
        s.settings
            .with(|st| st.as_ref().map_or(40.0, |st| st.threshold_warning_pct))
    });
    // Liest einen Wert des Geräts als Text: Das Element bleibt stehen, nur der Text ändert sich.
    let text = move |pick: fn(&Device) -> String| {
        Signal::derive(move || device.with(|d| d.as_ref().map(pick).unwrap_or_default()))
    };
    let status = Memo::new(move |_| device.with(|d| d.as_ref().map_or(DeviceStatus::Offline, |d| d.status)));
    let source = Memo::new(move |_| device.with(|d| d.as_ref().map_or(DataSource::Simulation, |d| d.data_source)));
    let soc = Memo::new(move |_| device.with(|d| d.as_ref().and_then(|d| d.soc_percent)));
    let route_href = Memo::new(move |_| {
        device.with(|d| {
            d.as_ref().and_then(|d| match (d.lat, d.lon) {
                (Some(lat), Some(lon)) => Some(format!(
                    "https://www.google.com/maps/dir/?api=1&destination={lat:.6},{lon:.6}"
                )),
                _ => None,
            })
        })
    });
    let alert_kinds = {
        let id = id.clone();
        Memo::new(move |_| {
            s.alerts.with(|list| {
                list.iter()
                    .filter(|a| a.device_id == id && a.status == AlertStatus::Open)
                    .map(|a| a.kind)
                    .collect::<Vec<AlertKind>>()
            })
        })
    };
    let planned: Memo<Option<MaintenanceTask>> = {
        let id = id.clone();
        Memo::new(move |_| s.planned_task(&id))
    };
    let sentence = move || {
        device.with(|d| {
            d.as_ref()
                .map(|d| status_sentence(d, critical.get()))
                .unwrap_or_default()
        })
    };
    let forecast_text = move || {
        device.with(|d| {
            let d = d.as_ref()?;
            let (rate, hours, soc) = (d.discharge_rate_pct_per_hour?, d.forecast_hours_remaining?, d.soc_percent?);
            let to_critical = ((soc - critical.get()) / rate).max(0.0);
            Some(format!(
                "Bei gleichbleibender Entladung ({} Prozentpunkte pro Stunde) ist die Batterie in {} leer und in {} unter dem kritischen Wert.",
                format::number(f64::from(rate), 2),
                format::duration_hours(hours),
                format::duration_hours(to_critical),
            ))
        })
        .unwrap_or_else(|| "Noch keine Prognose: Es fehlen Messwerte oder das Gerät wird gerade nicht entladen.".to_owned())
    };
    let seen_hint = Signal::derive(move || {
        device
            .with(|d| d.as_ref().and_then(|d| d.last_seen_at))
            .map(|t| format::age((s.now_ms.get() - t.timestamp_millis() as f64) as i64))
            .unwrap_or_default()
    });
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
    let share_id = id.clone();
    let swap_id = id.clone();
    let pin_id = id.clone();
    let sub_id = id.clone();
    let eff_id = id.clone();
    let page_id = id.clone();
    let history_id = id.clone();
    let task_device = id.clone();
    view! {
        <article class=if full { "device device-page" } else { "device device-panel" }>
            <header class="device-head">
                {full
                    .then(|| {
                        view! {
                            <button class="back" on:click=move |_| s.go_page(Page::Overview)>
                                <Icon name="arrow-left"/>
                                "Übersicht"
                            </button>
                        }
                    })}
                <div class="device-title">
                    <h1 class="device-id">{id.clone()}</h1>
                    {move || view! { <StatusDot status=status.get()/> }}
                    {move || view! { <Origin source=source.get()/> }}
                </div>
                <p class="device-sentence">{sentence}</p>
                <div class="device-actions">
                    <SwapButton device_id=swap_id.clone() class="btn-primary"/>
                    <PinButton id=pin_id.clone() label=true/>
                    <SubscribeButton id=sub_id.clone()/>
                    {move || {
                        route_href
                            .get()
                            .map(|href| {
                                view! {
                                    <a class="btn" href=href target="_blank" rel="noopener">
                                        <Icon name="map-pin"/>
                                        "Route"
                                    </a>
                                }
                            })
                    }}
                    <DeviceShare id=share_id.clone() csv_url=Signal::derive(move || api::export_url(&export_id, range.get()))/>
                    {(!full)
                        .then(|| {
                            view! {
                                <button class="btn" on:click=move |_| s.open_device_page(&page_id)>
                                    <Icon name="arrow-up-right"/>
                                    "Große Ansicht"
                                </button>
                            }
                        })}
                </div>
            </header>
            {move || {
                alert_kinds
                    .get()
                    .into_iter()
                    .map(|kind| {
                        let tone = match kind {
                            AlertKind::Critical => "diag crit",
                            AlertKind::Offline | AlertKind::Fault => "diag off",
                            AlertKind::Stagnation | AlertKind::Drop => "diag warn",
                        };
                        view! {
                            <div class=tone role="alert">
                                <strong>{kind.label()}</strong>
                                <span>{alert_hint(kind)}</span>
                            </div>
                        }
                    })
                    .collect_view()
            }}
            <section class="vitals" aria-label="Telemetrie">
                <div class=move || format!("vital-ring st-{}", format::status_class(status.get()))>
                    {move || {
                        s.on(Feature::Ring)
                            .then(|| {
                                view! {
                                    <svg class="big-ring" viewBox="0 0 120 120" role="img" aria-label="Ladezustand">
                                        <circle class="ring-bg" cx="60" cy="60" r="52" fill="none"></circle>
                                        <circle
                                            class="ring-fg"
                                            cx="60"
                                            cy="60"
                                            r="52"
                                            fill="none"
                                            stroke-dasharray=move || {
                                                let (dash, whole) = insights::ring_dash(soc.get().unwrap_or(0.0), 52.0);
                                                format!("{dash:.1} {whole:.1}")
                                            }
                                            transform="rotate(-90 60 60)"
                                        ></circle>
                                    </svg>
                                }
                            })
                    }}
                    <div class="vital-center">
                        <span class="vital-value mono-num">
                            {move || soc.get().map_or_else(|| "unbekannt".to_owned(), format::percent)}
                        </span>
                        <span class="vital-label">"Ladestand"</span>
                    </div>
                </div>
                <div class="figures">
                    <Figure label="Spannung" value=text(|d| d.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage))/>
                    <Figure label="Entladestrom" value=text(|d| d.current_ma.map_or_else(|| "unbekannt".to_owned(), format::current))/>
                    <Figure
                        label="Temperatur"
                        value=text(|d| d.temperature_c.map_or_else(|| "wird nicht gesendet".to_owned(), format::temperature))
                    />
                    <Figure
                        label="Position"
                        value=text(|d| match (d.lat, d.lon) {
                            (Some(lat), Some(lon)) => format!("{}, {}", format::coordinate(lat), format::coordinate(lon)),
                            _ => "Kein GPS-Fix".to_owned(),
                        })
                        hint=text(|d| d.satellites.map(|n| format!("{n} Satelliten")).unwrap_or_default())
                    />
                    <Figure
                        label="Letzter Kontakt"
                        value=text(|d| d.last_seen_at.map_or_else(|| "noch keiner".to_owned(), jsutil::date_time_label))
                        hint=seen_hint
                    />
                    <Figure
                        label="Batterie seit"
                        value=text(|d| d.battery_installed_at.map_or_else(|| "unbekannt".to_owned(), jsutil::date_label))
                        hint=text(|d| format!("{} Ah entnommen", format::number(d.discharged_ah, 1)))
                    />
                </div>
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
                        <HistoryChart history=history error=history_error critical=critical warning=warning/>
                        <p class="forecast-text">{forecast_text}</p>
                    </div>
                </section>
                <aside class="device-side">
                    <section class="surface">
                        <div class="surface-head"><h2>"Einsatz"</h2></div>
                        <div class="surface-body">
                            {move || match planned.get() {
                                Some(task) => {
                                    let task_id = task.id;
                                    let device_id = task_device.clone();
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
                                            <button class="btn" on:click=move |_| s.modal.set(Some(ModalKind::Ticket { task_id }))>
                                                "Einsatz öffnen"
                                            </button>
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
                            }}
                        </div>
                    </section>
                    <section class="surface">
                        <div class="surface-head"><h2>"Protokoll"</h2></div>
                        <div class="surface-body">
                            <History device=history_id limit=if full { 30 } else { 8 }/>
                        </div>
                    </section>
                </aside>
            </div>
            <EfficiencySection id=eff_id/>
        </article>
    }
}

/// Gemeinsame Ansicht. `full` ist die eigene Seite, sonst das Blatt am Rand.
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
    let exists = Memo::new(move |_| device.with(Option::is_some));
    view! {
        <Show
            when=move || exists.get()
            fallback=move || {
                view! {
                    <div class="empty-state">
                        <p class="empty">
                            {move || if s.loaded.get() { "Dieses Gerät gibt es nicht (mehr)." } else { "Daten werden geladen." }}
                        </p>
                        {move || {
                            (full && s.loaded.get())
                                .then(|| view! { <button class="btn" on:click=move |_| s.go_page(Page::Overview)>"Zur Übersicht"</button> })
                        }}
                    </div>
                }
            }
        >
            <DeviceBody id=id.clone() full=full device=device range=range history=history history_error=history_error/>
        </Show>
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

/// Blatt über Übersicht oder Karte.
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
    use chrono::DateTime;

    fn device(status: DeviceStatus, soc: Option<f32>, rate: Option<f32>, hours: Option<f32>) -> Device {
        Device {
            id: "X".into(),
            name: None,
            status,
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
            discharge_rate_pct_per_hour: rate,
            forecast_hours_remaining: hours,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
        }
    }

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

    #[test]
    fn the_status_sentence_names_status_and_remaining_time() {
        let critical = device(DeviceStatus::Critical, Some(10.0), Some(2.0), Some(5.0));
        assert_eq!(
            status_sentence(&critical, 20.0),
            format!("Kritisch. Noch {} bis leer.", format::duration_hours(5.0))
        );
        let fine = device(DeviceStatus::Normal, Some(60.0), Some(2.0), Some(30.0));
        assert_eq!(
            status_sentence(&fine, 20.0),
            format!("In Ordnung. Noch {} bis kritisch.", format::duration_hours(20.0))
        );
        assert_eq!(
            status_sentence(&device(DeviceStatus::Normal, Some(60.0), None, None), 20.0),
            "In Ordnung."
        );
        assert_eq!(
            status_sentence(&device(DeviceStatus::Offline, Some(60.0), Some(2.0), Some(30.0)), 20.0),
            "Offline. Es kommen keine Messwerte mehr an."
        );
        assert!(status_sentence(&device(DeviceStatus::Warning, Some(30.0), None, None), 20.0).starts_with("Warnung."));
        assert!(status_sentence(&device(DeviceStatus::Critical, None, None, None), 20.0).starts_with("Kritisch."));
    }
}
