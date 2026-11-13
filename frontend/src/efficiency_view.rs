//! Effizienz: Wie viel leistet eine Batterie wirklich, verglichen mit ihrer Nennkapazität?
//!
//! Die Flottenseite reiht die Geräte nach ihrer Effizienz, die Sektion im Gerät zeigt Kennzahlen,
//! den Einfluss von Kälte und Last und den Vergleich mit früheren Batteriezyklen. Alle Werte sind
//! Schätzungen aus Strom und Spannung, siehe `shared::efficiency`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::efficiency::{
    CycleSummary, DeviceEfficiency, EfficiencyDetail, LoadSplit, TemperatureBand, band_label, cold_effect_mv,
    health_vs_previous, load_effect_mv, score_label,
};

use crate::device::{DeviceLink, Figure};
use crate::overview::StatusDot;
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
    score.map_or("zu wenige Daten", score_label)
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
                                view! {
                                    <li class="eff-row">
                                        <div class="eff-main">
                                            <DeviceLink id=d.device_id.clone() class="eff-id mono-num">
                                                {format::device_title(&d.device_id, d.name.as_deref())}
                                            </DeviceLink>
                                            <StatusDot status=d.status/>
                                        </div>
                                        <div class="eff-score">
                                            <strong class="mono-num">{score_text(d.result.score_pct)}</strong>
                                            <span class="muted">{score_note(d.result.score_pct)}</span>
                                        </div>
                                        <div class="eff-bar" aria-hidden="true">
                                            <div class="eff-fill" style=format!("width: {share:.0}%")></div>
                                        </div>
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
                            </div>
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
        assert_eq!(score_note(None), "zu wenige Daten");
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
    fn the_voltage_bar_spans_empty_to_full() {
        assert_eq!(voltage_share(10_500.0, 10_500, 12_700), 0.0);
        assert_eq!(voltage_share(12_700.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_600.0, 10_500, 12_700), 50.0);
        assert_eq!(voltage_share(15_000.0, 10_500, 12_700), 100.0);
        assert_eq!(voltage_share(11_000.0, 12_000, 12_000), 0.0);
    }
}
