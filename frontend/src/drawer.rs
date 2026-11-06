//! Schublade mit allen Angaben zu einem Gerät, Verlauf, Prognose, Export und Batteriewechsel.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::NewMaintenanceTask;
use shared::api::{HistoryResponse, Range};

use crate::overview::Pill;
use crate::state::Store;
use crate::{api, chart, format, jsutil};

#[component]
fn Tile(label: &'static str, value: String, #[prop(optional, into)] hint: String) -> impl IntoView {
    view! {
        <div class="tile">
            <div class="tile-label">{label}</div>
            <div class="tile-value">{value}</div>
            {(!hint.is_empty()).then(|| view! { <div class="tile-hint muted">{hint}</div> })}
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
    view! {
        <svg class="chart" viewBox=view_box role="img" aria-label="Verlauf des Ladezustands in Prozent">
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
        </svg>
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
            let new = NewMaintenanceTask {
                device_id: id.clone(),
                alert_id: None,
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
    let confirm = {
        let planned = planned.clone();
        move |_| {
            let Some(task) = planned() else { return };
            spawn_local(async move {
                match api::complete_task(task.id).await {
                    Ok(_) => {
                        s.say("Batteriewechsel bestätigt. Der Ladezustand wird mit der nächsten Messung neu bestimmt.")
                    }
                    Err(e) => s.say(e),
                }
                s.refresh_soon();
            });
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
                        <Pill status=d.status/>
                    </header>
                    <section class="tiles">
                        <Tile label="Ladestand" value=d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)/>
                        <Tile label="Spannung" value=d.voltage_mv.map_or_else(|| "unbekannt".to_owned(), format::voltage)/>
                        <Tile label="Strom" value=d.current_ma.map_or_else(|| "unbekannt".to_owned(), format::current)/>
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
                    </section>
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
                            "CSV exportieren"
                        </a>
                    </section>
                    <section class="maint">
                        <h3>"Batteriewechsel"</h3>
                        {
                            let plan = plan.clone();
                            let confirm = confirm.clone();
                            let planned = planned.clone();
                            move || match planned() {
                                Some(task) => {
                                    view! {
                                        <p>{format!("Geplant seit {}.", jsutil::date_label(task.created_at))}</p>
                                        <button class="btn primary" on:click=confirm.clone()>"Batterie gewechselt"</button>
                                    }
                                        .into_any()
                                }
                                None => {
                                    view! {
                                        <p class="muted">"Noch kein Wechsel geplant."</p>
                                        <button class="btn primary" on:click=plan.clone()>"Batteriewechsel planen"</button>
                                    }
                                        .into_any()
                                }
                            }
                        }
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
                            <button class="drawer-close" aria-label="Schließen" on:click=move |_| close()>
                                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 5l14 14M19 5L5 19"></path></svg>
                            </button>
                            <DrawerBody id=id/>
                        </aside>
                    }
                })
        }}
    }
}
