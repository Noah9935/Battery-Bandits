//! Effizienz: Wie viel leistet eine Batterie wirklich, verglichen mit ihrer Nennkapazität?
//!
//! Die Flottenseite reiht die Geräte nach ihrer Effizienz, die Sektion im Gerät zeigt Kennzahlen,
//! den Einfluss von Kälte und Last und den Vergleich mit früheren Batteriezyklen. Alle Werte sind
//! Schätzungen aus Strom und Spannung, siehe `shared::efficiency`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::efficiency::{
    CycleSummary, DeviceEfficiency, EfficiencyDetail, LoadSplit, ModelSummary, REFERENCE_CURRENT_MA, Reason,
    TemperatureBand, band_label, cold_effect_mv, cost_per_day_cents, drop_share_pct, health_vs_previous,
    load_effect_mv, resistance_label, score_label, voltage_drop_mv,
};

use crate::device::{DeviceLink, Figure};
use crate::overview::{SimBadge, StatusDot};
use crate::state::Store;
use crate::{api, format, jsutil};

/// Mindestzahl Messwerte je Bereich, damit ein Vergleich etwas aussagt.
pub const MIN_READINGS: i64 = 10;

pub fn score_text(score: Option<f32>) -> String {
    score.map_or_else(
        || "keine Aussage".to_owned(),
        |s| format!("{} %", format::number(f64::from(s), 0)),
    )
}

pub fn score_note(score: Option<f32>) -> &'static str {
    score.map_or("keine Aussage möglich", score_label)
}

/// Satz unter einer fehlenden Effizienz: Warum es keine Aussage gibt. Leer, wenn es eine gibt.
pub fn reason_text(reason: Option<Reason>) -> &'static str {
    reason.map_or("", Reason::text)
}

pub fn drop_text(mv_per_day: Option<f64>) -> String {
    match mv_per_day {
        None => "unbekannt".to_owned(),
        Some(mv) if mv < 0.0 => format!("steigt um {} mV pro Tag", format::number(-mv, 0)),
        Some(mv) => format!("{} mV pro Tag", format::number(mv, 0)),
    }
}

pub fn runtime_text(hours: Option<f64>) -> String {
    hours.map_or_else(|| "unbekannt".to_owned(), format::runtime)
}

pub fn current_text(ma: Option<f64>) -> String {
    ma.map_or_else(|| "unbekannt".to_owned(), |ma| format!("{} mA", format::number(ma, 0)))
}

/// Geräte nach Effizienz, das schwächste zuerst. Geräte ohne Aussage stehen am Ende.
pub fn ranked(list: &[DeviceEfficiency]) -> Vec<DeviceEfficiency> {
    let mut sorted = list.to_vec();
    sorted.sort_by(|a, b| {
        let key = |d: &DeviceEfficiency| d.result.score_pct.unwrap_or(f32::MAX);
        key(a).total_cmp(&key(b)).then_with(|| a.device_id.cmp(&b.device_id))
    });
    sorted
}

/// Mittlere Effizienz aller Geräte, für die es eine Aussage gibt.
pub fn fleet_average(list: &[DeviceEfficiency]) -> Option<f32> {
    let scores: Vec<f32> = list.iter().filter_map(|d| d.result.score_pct).collect();
    (!scores.is_empty()).then(|| scores.iter().sum::<f32>() / scores.len() as f32)
}

/// Einfluss der Kälte in einem Satz.
pub fn cold_text(bands: &[TemperatureBand]) -> String {
    match cold_effect_mv(bands, MIN_READINGS) {
        Some(mv) if mv >= 50.0 => format!(
            "Bei Kälte liegt die Spannung im Mittel {} mV tiefer als in den warmen Stunden. Kälte kostet hier Leistung.",
            format::number(mv, 0)
        ),
        Some(mv) if mv <= -50.0 => "Bei Kälte liegt die Spannung nicht tiefer als bei Wärme.".to_owned(),
        Some(_) => "Kein spürbarer Einfluss der Temperatur auf die Spannung.".to_owned(),
        None => "Für einen Vergleich fehlen Messwerte aus mindestens zwei Temperaturbereichen.".to_owned(),
    }
}

/// Einfluss hoher Last (Senden) in einem Satz.
pub fn load_text(load: Option<&LoadSplit>) -> String {
    let Some(load) = load else {
        return "Noch keine Messwerte für einen Lastvergleich.".to_owned();
    };
    match load_effect_mv(load, MIN_READINGS) {
        Some(mv) if mv >= 50.0 => format!(
            "Unter hoher Last (mehr als das {}fache des üblichen Stroms von {} mA) fällt die Spannung im Mittel um {} mV.",
            format::number(shared::efficiency::HIGH_LOAD_FACTOR, 1),
            format::number(load.median_current_ma, 0),
            format::number(mv, 0)
        ),
        Some(_) => "Hohe Last drückt die Spannung kaum.".to_owned(),
        None => "Zu wenige Messwerte unter hoher Last für einen Vergleich.".to_owned(),
    }
}

/// Gesundheit im Vergleich zu früheren Batterien in einem Satz.
pub fn health_text(cycles: &[CycleSummary]) -> String {
    match health_vs_previous(cycles) {
        Some(pct) => format!(
            "Die laufende Batterie erreicht {} der Effizienz früherer Batterien dieses Geräts.",
            score_text(Some(pct))
        ),
        None => "Noch kein Vergleich: Es fehlt ein abgeschlossener Zyklus mit Messwerten.".to_owned(),
    }
}

/// Betrag in Euro aus Cent als Kommazahl, `keine Aussage` ohne Wert.
pub fn euro_or_none(cents: Option<f64>) -> String {
    cents.map_or_else(|| "keine Aussage".to_owned(), |c| shared::euro_text(c.round() as i32))
}

/// Die Kennzahlen eines Modells als Zeilen für die Liste: wie lange, wie viel, was es kostet.
pub fn model_facts(m: &ModelSummary) -> Vec<String> {
    let per_day = m.ah_per_day.map_or_else(
        || "keine Aussage".to_owned(),
        |a| format!("{} Ah pro Tag", format::number(a, 1)),
    );
    vec![
        format!(
            "Ø Laufzeit: {}",
            m.avg_hours.map_or_else(|| "keine Aussage".to_owned(), format::runtime)
        ),
        format!("Ø Verbrauch: {per_day}"),
        format!(
            "Ø Preis: {}",
            m.avg_price_cents
                .map_or_else(|| "nicht eingetragen".to_owned(), shared::euro_text)
        ),
        format!("Kosten je Ah: {}", euro_or_none(m.cost_per_ah_cents)),
        format!(
            "Ø Innenwiderstand: {}",
            m.avg_resistance_mohm.map_or_else(
                || "keine Aussage".to_owned(),
                |r| format!("{} mΩ ({})", format::number(r, 1), resistance_label(r))
            )
        ),
        format!("Ø Effizienz: {}", score_text(m.avg_score_pct)),
    ]
}

/// Überschrift einer Modellzeile: `3 Batterien, 2 abgeschlossen`.
pub fn model_count_text(m: &ModelSummary) -> String {
    let word = if m.batteries == 1 { "Batterie" } else { "Batterien" };
    format!("{} {word}, {} abgeschlossen", m.batteries, m.finished)
}

/// Spannungsabfall unter Last, so wie man ihn im Alltag rechnet (`U = I * R`): Aus dem Innenwiderstand folgt, wie viel
/// Klemmenspannung bei dem mittleren Strom und beim Senden mit 5 A verloren geht.
pub fn drop_calc_text(resistance_mohm: Option<f64>, avg_current_ma: Option<f64>, terminal_mv: f64) -> String {
    let Some(r) = resistance_mohm else {
        return "Innenwiderstand: keine Aussage. Dafür muss der Strom unterschiedlich stark sein (Ruhe und Senden) und es braucht mindestens 100 Messwerte.".to_owned();
    };
    let at_send = voltage_drop_mv(r, REFERENCE_CURRENT_MA);
    let share = drop_share_pct(at_send, terminal_mv).map_or_else(String::new, |p| {
        format!(" (ein Anteil von {} Prozent der Klemmenspannung)", format::number(p, 1))
    });
    let at_avg = avg_current_ma.map_or_else(String::new, |ma| {
        format!(
            " Beim mittleren Strom von {} fällt die Klemmenspannung um {}.",
            current_text(Some(ma)),
            drop_text_mv(voltage_drop_mv(r, ma))
        )
    });
    format!(
        "Innenwiderstand etwa {} mΩ, das ist {}. U = I mal R:{at_avg} Beim Senden mit 5 A wären es {}{share}.",
        format::number(r, 1),
        resistance_label(r),
        drop_text_mv(at_send)
    )
}

fn drop_text_mv(mv: f64) -> String {
    if mv < 10.0 {
        format!("{} mV", format::number(mv, 1))
    } else {
        format!("{} mV", format::number(mv, 0))
    }
}

/// Modell und Preis einer Batterie in einer Zeile, `None` ohne beides.
pub fn battery_line(model: Option<&str>, price_cents: Option<i32>, hours: f64) -> Option<String> {
    let model = model.map(str::trim).filter(|m| !m.is_empty());
    match (model, price_text(price_cents, hours)) {
        (None, None) => None,
        (Some(m), None) => Some(m.to_owned()),
        (None, Some(p)) => Some(p),
        (Some(m), Some(p)) => Some(format!("{m}, {p}")),
    }
}

/// Preis und Kosten je Tag einer Batterie in einer Zeile, `None` ohne Preis.
pub fn price_text(price_cents: Option<i32>, hours: f64) -> Option<String> {
    let price = price_cents?;
    Some(match cost_per_day_cents(price_cents, hours) {
        Some(per_day) => format!(
            "{}, {} pro Tag",
            shared::euro_text(price),
            shared::euro_text(per_day.round() as i32)
        ),
        None => shared::euro_text(price),
    })
}

/// Anteil der Spannungsspanne (leer bis voll) für den Balken.
pub fn voltage_share(mv: f64, min_mv: i32, max_mv: i32) -> f64 {
    let span = f64::from(max_mv) - f64::from(min_mv);
    if span <= 0.0 {
        return 0.0;
    }
    ((mv - f64::from(min_mv)) / span * 100.0).clamp(0.0, 100.0)
}

/// Seite "Effizienz": Datenbestand und die Geräte der Flotte nach ihrer Effizienz.
#[component]
pub fn EfficiencyPage() -> impl IntoView {
    let s = expect_context::<Store>();
    // Die Seite wird erst beim ersten Öffnen aufgebaut: Dann laden.
    s.load_efficiency();
    let list = Memo::new(move |_| {
        s.efficiency
            .with(|e| e.as_ref().map(|e| ranked(&e.devices)).unwrap_or_default())
    });
    let average = Memo::new(move |_| {
        s.efficiency
            .with(|e| e.as_ref().and_then(|e| fleet_average(&e.devices)))
    });
    view! {
        <div class="view">
            <header class="view-head">
                <h1>"Effizienz"</h1>
                <p class="sub">
                    "Wie viel leistet jede Batterie wirklich? Verglichen werden Spannungsabfall und Strom mit der Nennkapazität."
                </p>
            </header>
            {move || s.efficiency_error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <section class="surface stock">
                <div class="surface-head">
                    <h2>"Datenhaltung"</h2>
                </div>
                <div class="surface-body stock-grid">
                    {move || {
                        s.efficiency
                            .get()
                            .map(|e| {
                                let stock = e.stock;
                                view! {
                                    <Figure label="Gespeicherte Messwerte" value=format::number(stock.readings_total as f64, 0)/>
                                    <Figure
                                        label="Ältester Messwert"
                                        value=stock.oldest_reading.map_or_else(|| "noch keiner".to_owned(), jsutil::date_label)
                                    />
                                    <Figure
                                        label="Neuester Messwert"
                                        value=stock.newest_reading.map_or_else(|| "noch keiner".to_owned(), jsutil::date_time_label)
                                    />
                                    <Figure
                                        label="Verworfene Nachrichten"
                                        value=format::number(stock.invalid_messages as f64, 0)
                                        hint="mit Grund protokolliert".to_owned()
                                    />
                                }
                            })
                    }}
                </div>
            </section>
            <section class="surface">
                <div class="surface-head">
                    <h2>"Nach Batteriemodell"</h2>
                    <span class="muted">"Gleiche Modelle zusammengenommen"</span>
                </div>
                <ul class="eff-list">
                    {move || {
                        let models = s.efficiency.with(|e| e.as_ref().map(|e| e.models.clone()).unwrap_or_default());
                        if models.is_empty() {
                            return view! { <li class="empty">"Noch keine Batterien erfasst. Das Modell trägt man beim Batteriewechsel ein."</li> }.into_any();
                        }
                        models
                            .into_iter()
                            .map(|m| {
                                let facts = model_facts(&m);
                                view! {
                                    <li class="eff-row">
                                        <div class="eff-main">
                                            <strong class="eff-id">{m.model.clone()}</strong>
                                            <span class="muted">{model_count_text(&m)}</span>
                                        </div>
                                        <div class="eff-score">
                                            <strong class="mono-num">{euro_or_none(m.cost_per_day_cents)}</strong>
                                            <span class="muted">"pro Tag"</span>
                                        </div>
                                        <div class="eff-facts muted mono-num">
                                            {facts.into_iter().map(|f| view! { <span>{f}</span> }).collect_view()}
                                        </div>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
            </section>
            <section class="surface">
                <div class="surface-head">
                    <h2>"Flotte nach Effizienz"</h2>
                    <span class="muted mono-num">
                        {move || {
                            average.get().map_or_else(
                                || "noch keine Aussage".to_owned(),
                                |a| format!("Mittel {}", score_text(Some(a))),
                            )
                        }}
                    </span>
                </div>
                <ul class="eff-list">
                    {move || {
                        let rows = list.get();
                        if rows.is_empty() {
                            return view! { <li class="empty">"Noch keine Geräte mit Messwerten."</li> }.into_any();
                        }
                        rows.into_iter()
                            .map(|d| {
                                let share = d.result.score_pct.map_or(0.0, |v| f64::from(v).min(100.0));
                                let badge_id = d.device_id.clone();
                                let title = format::device_title(&d.device_id, d.name.as_deref());
                                view! {
                                    <li class="eff-row">
                                        <div class="eff-main">
                                            <DeviceLink id=d.device_id.clone() class="eff-id mono-num">
                                                {title}
                                            </DeviceLink>
                                            <StatusDot status=d.status/>
                                            <SimBadge id=badge_id/>
                                        </div>
                                        <div class="eff-score">
                                            <strong class="mono-num">{score_text(d.result.score_pct)}</strong>
                                            <span class="muted">{score_note(d.result.score_pct)}</span>
                                        </div>
                                        <div class="eff-bar" aria-hidden="true">
                                            <div class="eff-fill" style=format!("width: {share:.0}%")></div>
                                        </div>
                                        <p class="eff-reason muted">{reason_text(d.result.reason)}</p>
                                        <div class="eff-facts muted mono-num">
                                            <span>{current_text(d.result.avg_current_ma)}</span>
                                            <span>{drop_text(d.result.voltage_drop_mv_per_day)}</span>
                                            <span>{format!("real: {}", runtime_text(d.result.real_runtime_hours))}</span>
                                            <span>{format!("nominal: {}", runtime_text(d.result.nominal_runtime_hours))}</span>
                                        </div>
                                    </li>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </ul>
            </section>
            <p class="legend">
                "Schätzung: Die Spannung fällt zwischen voll und leer näherungsweise linear. Gemessen wird in den letzten 30 Tagen, höchstens seit dem letzten Batteriewechsel."
            </p>
        </div>
    }
}

/// Effizienz-Sektion in der Ansicht eines Geräts.
#[component]
pub fn EfficiencySection(id: String) -> impl IntoView {
    let s = expect_context::<Store>();
    let detail = RwSignal::new(None::<EfficiencyDetail>);
    let error = RwSignal::new(None::<String>);
    {
        let id = id.clone();
        spawn_local(async move {
            match api::device_efficiency(&id).await {
                Ok(d) => detail.set(Some(d)),
                Err(e) => error.set(Some(e)),
            }
        });
    }
    let volts = move || {
        s.settings.with(|st| {
            st.as_ref()
                .map_or((10_500, 12_700), |st| (st.min_voltage_mv, st.max_voltage_mv))
        })
    };
    view! {
        <section class="surface device-eff">
            <div class="surface-head">
                <h2>"Effizienz"</h2>
            </div>
            <div class="surface-body">
                {move || error.get().map(|e| view! { <p class="empty">{e}</p> })}
                {move || {
                    let Some(d) = detail.get() else {
                        return (error.with(Option::is_none)).then(|| view! { <p class="empty">"Effizienz wird berechnet."</p> }.into_any());
                    };
                    let r = d.current.result;
                    let (min_mv, max_mv) = volts();
                    let terminal_mv = s
                        .device(&d.current.device_id)
                        .and_then(|dev| dev.voltage_mv)
                        .map_or(12_000.0, f64::from);
                    let bands = d.bands.clone();
                    Some(
                        view! {
                            <div class="figures">
                                <Figure label="Effizienz" value=score_text(r.score_pct) hint=score_note(r.score_pct).to_owned()/>
                                <Figure label="Mittlerer Strom" value=current_text(r.avg_current_ma)/>
                                <Figure label="Spannungsabfall" value=drop_text(r.voltage_drop_mv_per_day)/>
                                <Figure
                                    label="Laufzeit laut Nennkapazität"
                                    value=runtime_text(r.nominal_runtime_hours)
                                />
                                <Figure label="Reale Laufzeit" value=runtime_text(r.real_runtime_hours)/>
                                <Figure
                                    label="Innenwiderstand"
                                    value=d.current.internal_resistance_mohm.map_or_else(|| "keine Aussage".to_owned(), |v| format!("{} mΩ", format::number(v, 1)))
                                    hint=d.current.internal_resistance_mohm.map(|v| resistance_label(v).to_owned()).unwrap_or_default()
                                />
                            </div>
                            <h3>"Spannungsabfall unter Last"</h3>
                            <p class="forecast-text">{drop_calc_text(d.current.internal_resistance_mohm, r.avg_current_ma, terminal_mv)}</p>
                            {r.reason.map(|reason| view! { <p class="forecast-text">{reason.text()}</p> })}
                            <h3>"Temperatur"</h3>
                            <ul class="band-list">
                                {bands
                                    .iter()
                                    .map(|b| {
                                        let share = voltage_share(b.avg_voltage_mv, min_mv, max_mv);
                                        view! {
                                            <li class="band-row">
                                                <span class="band-name">{band_label(b.from_c, b.to_c)}</span>
                                                <span class="eff-bar" aria-hidden="true">
                                                    <span class="eff-fill" style=format!("width: {share:.0}%")></span>
                                                </span>
                                                <span class="mono-num">{format::voltage(b.avg_voltage_mv.round() as i32)}</span>
                                                <span class="muted mono-num">{format!("{} Messwerte", b.readings)}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                            <p class="forecast-text">{cold_text(&bands)}</p>
                            <h3>"Last beim Senden"</h3>
                            <p class="forecast-text">{load_text(d.load.as_ref())}</p>
                            <h3>"Batteriezyklen"</h3>
                            <p class="forecast-text">{health_text(&d.cycles)}</p>
                            <ul class="cycle-list">
                                {d
                                    .cycles
                                    .iter()
                                    .map(|c| {
                                        let share = c.result.score_pct.map_or(0.0, |v| f64::from(v).min(100.0));
                                        let when = match c.ended_at {
                                            None => format!("Laufend seit {}", jsutil::date_label(c.started_at)),
                                            Some(end) => format!(
                                                "{} bis {}",
                                                jsutil::date_label(c.started_at),
                                                jsutil::date_label(end)
                                            ),
                                        };
                                        view! {
                                            <li class="cycle-row">
                                                <span class="band-name">{when}</span>
                                                <span class="eff-bar" aria-hidden="true">
                                                    <span class="eff-fill" style=format!("width: {share:.0}%")></span>
                                                </span>
                                                <strong class="mono-num">{score_text(c.result.score_pct)}</strong>
                                                <span class="muted mono-num">
                                                    {format!(
                                                        "{}, {} Ah",
                                                        format::runtime(c.hours),
                                                        format::number(c.discharged_ah, 1)
                                                    )}
                                                </span>
                                                {battery_line(c.model.as_deref(), c.price_cents, c.hours)
                                                    .map(|text| view! { <span class="muted mono-num cycle-price">{text}</span> })}
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                            .into_any(),
                    )
                }}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::DeviceStatus;
    use shared::efficiency::Efficiency;

    fn eff(id: &str, score: Option<f32>) -> DeviceEfficiency {
        DeviceEfficiency {
            device_id: id.into(),
            name: None,
            status: DeviceStatus::Normal,
            window_hours: 48.0,
            readings: 100,
            avg_temperature_c: None,
            internal_resistance_mohm: None,
            result: Efficiency {
                score_pct: score,
                ..Efficiency::default()
            },
        }
    }

    fn band(readings: i64, mv: f64) -> TemperatureBand {
        TemperatureBand {
            from_c: None,
            to_c: Some(0.0),
            readings,
            avg_current_ma: 300.0,
            avg_voltage_mv: mv,
        }
    }

    #[test]
    fn scores_and_notes_read_naturally() {
        assert_eq!(score_text(Some(82.4)), "82 %");
        assert_eq!(score_text(None), "keine Aussage");
        assert_eq!(score_note(Some(95.0)), "wie neu");
        assert_eq!(score_note(None), "keine Aussage möglich");
        assert_eq!(reason_text(None), "");
        assert!(reason_text(Some(Reason::Implausible)).contains("120 Prozent"));
    }

    #[test]
    fn units_are_german_and_say_when_the_voltage_rises() {
        assert_eq!(drop_text(Some(528.4)), "528 mV pro Tag");
        assert_eq!(drop_text(Some(-40.0)), "steigt um 40 mV pro Tag");
        assert_eq!(drop_text(None), "unbekannt");
        assert_eq!(runtime_text(Some(36.0)), "36 Std.");
        assert_eq!(runtime_text(Some(300.0)), "12,5 Tage");
        assert_eq!(current_text(Some(812.3)), "812 mA");
        assert_eq!(current_text(None), "unbekannt");
    }

    #[test]
    fn the_weakest_battery_comes_first_and_unknown_ones_last() {
        let list = [
            eff("C", Some(90.0)),
            eff("U", None),
            eff("A", Some(40.0)),
            eff("B", Some(40.0)),
        ];
        let ids: Vec<String> = ranked(&list).into_iter().map(|d| d.device_id).collect();
        assert_eq!(ids, ["A", "B", "C", "U"]);
        assert_eq!(fleet_average(&list), Some((90.0 + 40.0 + 40.0) / 3.0));
        assert_eq!(fleet_average(&[eff("U", None)]), None);
        assert_eq!(fleet_average(&[]), None);
    }

    #[test]
    fn the_cold_sentence_depends_on_the_difference() {
        let cold = [
            band(40, 11_900.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_150.0)
            },
        ];
        assert!(cold_text(&cold).contains("250 mV tiefer"), "{}", cold_text(&cold));
        let flat = [
            band(40, 12_100.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_120.0)
            },
        ];
        assert_eq!(
            cold_text(&flat),
            "Kein spürbarer Einfluss der Temperatur auf die Spannung."
        );
        let reversed = [
            band(40, 12_300.0),
            TemperatureBand {
                from_c: Some(20.0),
                to_c: None,
                ..band(60, 12_100.0)
            },
        ];
        assert!(cold_text(&reversed).contains("nicht tiefer"));
        assert!(cold_text(&cold[..1]).contains("mindestens zwei"));
    }

    #[test]
    fn the_load_sentence_names_the_voltage_loss() {
        let load = LoadSplit {
            median_current_ma: 300.0,
            high_readings: 20,
            high_avg_voltage_mv: Some(11_800.0),
            normal_readings: 200,
            normal_avg_voltage_mv: Some(12_100.0),
        };
        let text = load_text(Some(&load));
        assert!(text.contains("300 mA") && text.contains("um 300 mV"), "{text}");
        assert!(load_text(None).contains("Noch keine"));
        let few = LoadSplit {
            high_readings: 2,
            ..load.clone()
        };
        assert!(load_text(Some(&few)).contains("Zu wenige"));
        let mild = LoadSplit {
            high_avg_voltage_mv: Some(12_090.0),
            ..load
        };
        assert_eq!(load_text(Some(&mild)), "Hohe Last drückt die Spannung kaum.");
    }

    #[test]
    fn health_is_compared_with_earlier_cycles() {
        let at = chrono::DateTime::from_timestamp(0, 0).unwrap();
        let cycle = |ended: bool, score: f32| CycleSummary {
            started_at: at,
            ended_at: ended.then_some(at),
            hours: 10.0,
            discharged_ah: 1.0,
            avg_temperature_c: None,
            price_cents: None,
            model: None,
            resistance_mohm: None,
            result: Efficiency {
                score_pct: Some(score),
                ..Efficiency::default()
            },
        };
        let text = health_text(&[cycle(false, 72.0), cycle(true, 90.0)]);
        assert_eq!(
            text,
            "Die laufende Batterie erreicht 80 % der Effizienz früherer Batterien dieses Geräts."
        );
        assert!(health_text(&[cycle(false, 72.0)]).starts_with("Noch kein Vergleich"));
    }

    #[test]
    fn a_price_is_shown_with_the_cost_per_day_once_the_run_is_long_enough() {
        assert_eq!(price_text(None, 240.0), None);
        assert_eq!(
            price_text(Some(9_600), 240.0).as_deref(),
            Some("96,00 €, 9,60 € pro Tag")
        );
        assert_eq!(price_text(Some(9_600), 6.0).as_deref(), Some("96,00 €"));
    }

    fn model(cost_per_day: Option<f64>) -> ModelSummary {
        ModelSummary {
            model: "Varta 80".into(),
            batteries: 3,
            finished: 2,
            avg_hours: Some(360.0),
            avg_discharged_ah: Some(50.0),
            ah_per_day: Some(3.33),
            avg_price_cents: Some(11_000),
            cost_per_day_cents: cost_per_day,
            cost_per_ah_cents: Some(220.0),
            avg_resistance_mohm: Some(9.4),
            avg_score_pct: Some(91.0),
        }
    }

    #[test]
    fn a_model_is_described_by_what_it_lasts_and_costs() {
        let m = model(Some(733.0));
        assert_eq!(model_count_text(&m), "3 Batterien, 2 abgeschlossen");
        assert_eq!(
            model_facts(&m),
            [
                "Ø Laufzeit: 15,0 Tage",
                "Ø Verbrauch: 3,3 Ah pro Tag",
                "Ø Preis: 110,00 €",
                "Kosten je Ah: 2,20 €",
                "Ø Innenwiderstand: 9,4 mΩ (gut)",
                "Ø Effizienz: 91 %",
            ]
        );
        assert_eq!(euro_or_none(Some(733.0)), "7,33 €");
        assert_eq!(euro_or_none(None), "keine Aussage");
        let bare = ModelSummary {
            avg_hours: None,
            ah_per_day: None,
            avg_price_cents: None,
            cost_per_ah_cents: None,
            avg_resistance_mohm: None,
            avg_score_pct: None,
            batteries: 1,
            ..model(None)
        };
        assert_eq!(model_count_text(&bare), "1 Batterie, 2 abgeschlossen");
        let facts = model_facts(&bare);
        assert!(
            facts
                .iter()
                .all(|f| f.ends_with("keine Aussage") || f.ends_with("nicht eingetragen")),
            "{facts:?}"
        );
    }

    #[test]
    fn the_voltage_drop_is_calculated_like_in_real_life() {
        // 10 mΩ: bei 306 mA fallen 3,1 mV, bei 5 A 50 mV, das sind 0,4 Prozent von 12 V.
        let text = drop_calc_text(Some(10.0), Some(306.0), 12_000.0);
        assert!(text.contains("etwa 10,0 mΩ, das ist gut"), "{text}");
        assert!(
            text.contains("von 306 mA fällt die Klemmenspannung um 3,1 mV"),
            "{text}"
        );
        assert!(
            text.contains("mit 5 A wären es 50 mV (ein Anteil von 0,4 Prozent"),
            "{text}"
        );
        let high = drop_calc_text(Some(40.0), None, 12_000.0);
        assert!(
            high.contains("das ist hoch") && !high.contains("mittleren Strom"),
            "{high}"
        );
        assert!(drop_calc_text(None, Some(306.0), 12_000.0).starts_with("Innenwiderstand: keine Aussage."));
    }

    #[test]
    fn a_battery_line_joins_model_and_price() {
        assert_eq!(battery_line(None, None, 240.0), None);
        assert_eq!(
            battery_line(Some(" Varta 80 "), None, 240.0).as_deref(),
            Some("Varta 80")
        );
        assert_eq!(
            battery_line(None, Some(9_600), 240.0).as_deref(),
            Some("96,00 €, 9,60 € pro Tag")
        );
        assert_eq!(
            battery_line(Some("Varta 80"), Some(9_600), 6.0).as_deref(),
            Some("Varta 80, 96,00 €")
        );
        assert_eq!(battery_line(Some("  "), None, 1.0), None);
    }

    #[test]
    fn the_voltage_bar_spans_empty_to_full() {
        assert_eq!(voltage_share(10_500.0, 10_500, 12_700), 0.0);
        assert_eq!(voltage_share(12_700.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_600.0, 10_500, 12_700), 50.0);
        assert_eq!(voltage_share(15_000.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_000.0, 12_000, 12_000), 0.0);
    }
}
