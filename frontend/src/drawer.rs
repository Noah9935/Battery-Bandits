//! Schublade mit allen Angaben zu einem Gerät, Verlauf, Prognose, Export und Batteriewechsel.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::{HistoryResponse, Range};

use crate::features::Feature;
use crate::history::History;
use crate::icons::Icon;
use crate::insights;
use crate::overview::{Origin, StatusDot};
use crate::state::{ModalKind, Store};
use crate::{api, chart, format, jsutil};
use wasm_bindgen::JsCast;

#[component]
fn Tile(label: &'static str, value: String, #[prop(optional, into)] hint: String) -> impl IntoView {
    view! {
        <div class="kv">
            <dt>{label}</dt>
            <dd class="tabular">
                <span class="mono">{value}</span>
                {(!hint.is_empty()).then(|| view! { <span class="kv-hint muted">{hint}</span> })}
            </dd>
        </div>
    }
}

/// Runde Anzeige des Ladezustands (neutral gefärbt, der Status steht als Punkt im Kopf).
fn ring(soc: Option<f32>) -> impl IntoView {
    let (dash, whole) = insights::ring_dash(soc.unwrap_or(0.0), 30.0);
    view! {
        <div class="ring">
            <svg viewBox="0 0 72 72" width="72" height="72" role="img" aria-label="Ladezustand">
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
            <span class="ring-text tabular">{soc.map_or_else(|| "unbekannt".to_owned(), format::percent)}</span>
        </div>
    }
}

#[component]
fn HistoryChart(history: HistoryResponse, critical_pct: f32) -> impl IntoView {
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
            <line class="threshold" x1=model.left x2=model.right y1=model.threshold_y y2=model.threshold_y></line>
            <text class="threshold-label" x=model.right y=model.threshold_y - 4.0 text-anchor="end">"kritisch"</text>
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
        <p class="cross-label tabular">
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
        <p class="legend muted">
            "Durchgezogen: Ladezustand. Gestrichelt: Prognose bei gleichbleibender Entladung. Waagerecht: kritischer Wert."
        </p>
    }
    .into_any()
}

#[component]
fn DrawerBody(id: String) -> impl IntoView {
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
    let planned = {
        let id = id.clone();
        move || s.planned_task(&id)
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
    view! {
        {move || match device.get() {
            None => view! { <p class="empty">"Dieses Gerät ist nicht mehr vorhanden."</p> }.into_any(),
            Some(d) => {
                let seen = d.last_seen_at;
                let export_id = export_id.clone();
                let forecast_text = {
                    let rate = d.discharge_rate_pct_per_hour;
                    let remaining = d.forecast_hours_remaining;
                    let soc = d.soc_percent;
                    match (rate, remaining, soc) {
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
                    }
                };
                view! {
                    <header class="drawer-head">
                        <div>
                            <h2>{d.name.clone().unwrap_or_else(|| d.id.clone())}</h2>
                            <p class="muted">{d.id.clone()}</p>
                        </div>
                        <StatusDot status=d.status/>
                    </header>
                    {s.on(Feature::Ring).then(|| ring(d.soc_percent))}
                    <dl class="kv-list">
                        <div class="kv">
                            <dt>"Herkunft"</dt>
                            <dd><Origin source=d.data_source/></dd>
                        </div>
                        <Tile label="Ladestand" value=d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)/>
                        <Tile label="Spannung" value=d.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage)/>
                        <Tile label="Entladestrom" value=d.current_ma.map_or_else(|| "unbekannt".to_owned(), format::current)/>
                        <Tile label="Temperatur" value=d.temperature_c.map_or_else(|| "unbekannt".to_owned(), format::temperature)/>
                        <Tile
                            label="Position"
                            value=match (d.lat, d.lon) {
                                (Some(lat), Some(lon)) => format!("{}, {}", format::coordinate(lat), format::coordinate(lon)),
                                _ => "Kein GPS-Fix".to_owned(),
                            }
                            hint=d.satellites.map(|n| format!("{n} Satelliten")).unwrap_or_default()
                        />
                        <Tile label="Höhe" value=d.altitude_m.map_or_else(|| "unbekannt".to_owned(), |a| format!("{} m", format::number(a, 0)))/>
                        <Tile
                            label="Letzte Meldung"
                            value=seen.map_or_else(|| "noch keine".to_owned(), jsutil::date_time_label)
                            hint=seen.map(|t| format::age((s.now_ms.get_untracked() - t.timestamp_millis() as f64) as i64)).unwrap_or_default()
                        />
                        <Tile
                            label="Aktuelle Batterie seit"
                            value=d.battery_installed_at.map_or_else(|| "unbekannt".to_owned(), jsutil::date_label)
                            hint=format!("{} Ah entnommen", format::number(d.discharged_ah, 1))
                        />
                    </dl>
                    <section class="forecast">
                        <h3>"Prognose"</h3>
                        <p>{forecast_text}</p>
                    </section>
                    <section class="history">
                        <div class="history-head">
                            <h3>"Verlauf"</h3>
                            <div class="seg" role="group" aria-label="Zeitraum">
                                {Range::ALL
                                    .into_iter()
                                    .map(|r| {
                                        view! {
                                            <button
                                                class=move || if range.get() == r { "seg-btn active" } else { "seg-btn" }
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
                        {move || {
                            if let Some(e) = history_error.get() {
                                return view! { <p class="empty">{e}</p> }.into_any();
                            }
                            match history.get() {
                                Some(h) => view! { <HistoryChart history=h critical_pct=critical_pct()/> }.into_any(),
                                None => view! { <p class="empty">"Verlauf wird geladen."</p> }.into_any(),
                            }
                        }}
                        <a class="btn" href=move || api::export_url(&export_id, range.get()) download="">
                            <Icon name="download"/>
                            "CSV exportieren"
                        </a>
                    </section>
                    {
                        let route = match (d.lat, d.lon) {
                            (Some(lat), Some(lon)) => Some(format!("https://www.google.com/maps/dir/?api=1&destination={lat:.6},{lon:.6}")),
                            _ => None,
                        };
                        route.map(|href| view! {
                            <a class="btn" href=href target="_blank" rel="noopener">
                                <Icon name="map-pin"/>
                                "Route in Google Maps"
                            </a>
                        })
                    }
                    <section class="maint">
                        <h3>"Einsatz"</h3>
                        {
                            let plan = plan.clone();
                            let planned = planned.clone();
                            move || match planned() {
                                Some(task) => {
                                    let task_id = task.id;
                                    view! {
                                        <p>
                                            {format!(
                                                "{} geplant seit {}. {}.",
                                                task.kind.label(),
                                                jsutil::date_label(task.created_at),
                                                crate::tickets::assignee_text(&task),
                                            )}
                                        </p>
                                        <button
                                            class="btn primary"
                                            on:click=move |_| s.modal.set(Some(ModalKind::Ticket { task_id }))
                                        >
                                            "Einsatz öffnen"
                                        </button>
                                    }
                                        .into_any()
                                }
                                None => {
                                    view! {
                                        <p class="muted">"Noch kein Einsatz geplant."</p>
                                        <button class="btn primary" on:click=plan.clone()>"Einsatz planen"</button>
                                    }
                                        .into_any()
                                }
                            }
                        }
                    </section>
                    <section class="maint">
                        <h3>"Protokoll"</h3>
                        <History device=d.id.clone() limit=8/>
                    </section>
                }
                    .into_any()
            }
        }}
    }
}

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
                            <DrawerBody id=id/>
                        </aside>
                    }
                })
        }}
    }
}
