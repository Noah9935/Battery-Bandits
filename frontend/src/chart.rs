//! Geometrie des Verlaufsdiagramms (SVG), rein und testbar.
//!
//! Die Zeitachse läuft vom ersten Messpunkt bis zum letzten, bei vorhandener Prognose zusätzlich ein
//! Stück in die Zukunft. Die Hochachse ist fest 0 bis 100 Prozent. Die gestrichelte waagerechte Linie
//! zeigt den kritischen Schwellwert. War ein Gerät eine Zeit lang nicht aktiv, reißt die Kurve dort ab:
//! Zwischen weit auseinanderliegenden Messpunkten gibt es weder Linie noch Fläche noch Fehlerband.

use shared::Forecast;
use shared::api::{HistoryPoint, Range};
use std::ops::Range as Span;

pub const WIDTH: f64 = 640.0;
pub const HEIGHT: f64 = 260.0;
const LEFT: f64 = 44.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const BOTTOM: f64 = 30.0;
/// Ränder der Zeichenfläche, für Linien, die außerhalb von `build` gezeichnet werden.
pub const PLOT_LEFT: f64 = LEFT;
pub const PLOT_RIGHT: f64 = RIGHT;
pub const PLOT_TOP: f64 = TOP;
pub const PLOT_BOTTOM: f64 = BOTTOM;

#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub position: f64,
    pub label: String,
}

/// Ein Messpunkt in Bildschirmkoordinaten des Diagramms (für das Fadenkreuz).
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPoint {
    pub x: f64,
    pub y: f64,
    pub time_ms: i64,
    pub soc: f32,
}

/// Index des Punkts, der `x` am nächsten liegt.
pub fn nearest(points: &[ChartPoint], x: f64) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1.x - x).abs().total_cmp(&(b.1.x - x).abs()))
        .map(|(i, _)| i)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartModel {
    pub width: f64,
    pub height: f64,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    /// Pfad der Ladezustandskurve, ein Teilstück je zusammenhängender Messreihe. Leer ohne Messpunkte und wenn
    /// nur einzelne Punkte vorliegen (dann siehe `dots`).
    pub soc_path: String,
    /// Dieselbe Kurve, je Teilstück unten zur Grundlinie geschlossen: Fläche mit Verlauf darunter.
    pub area_path: String,
    /// Einzelne Messpunkte, die zwischen zwei Lücken allein stehen und deshalb keine Linie bilden.
    pub dots: Vec<(f64, f64)>,
    /// Gestrichelte Prognose vom letzten Punkt bis zum Ende des Zeitfensters.
    pub forecast_path: Option<String>,
    /// Fehlerband (95 Prozent) um die Ladezustandskurve, nur wenn Unsicherheiten vorliegen.
    pub band_path: Option<String>,
    /// Trichter der Prognose: schnellste und langsamste Entladung innerhalb der Spanne.
    pub forecast_band_path: Option<String>,
    /// Hochachsen-Position des kritischen Schwellwerts.
    pub threshold_y: f64,
    pub y_ticks: Vec<Tick>,
    pub x_ticks: Vec<Tick>,
    /// Alle Messpunkte mit Position, für das Fadenkreuz.
    pub points: Vec<ChartPoint>,
}

impl ChartModel {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Länge des Zeitraums in Stunden. Bei "Gesamt" und einem eigenen Zeitraum ist es die Länge der Aufzeichnung
/// (mindestens ein Tag).
fn range_hours(range: Range, data_hours: f64) -> f64 {
    match range {
        Range::Day => 24.0,
        Range::Week => 7.0 * 24.0,
        Range::Month => 30.0 * 24.0,
        Range::All | Range::Custom => data_hours.max(24.0),
    }
}

/// Kürzester Abstand, ab dem eine Pause als Lücke gilt.
const MIN_GAP_MS: i64 = 15 * 60_000;

/// Ab diesem Abstand zweier Messpunkte gilt das Gerät als zwischenzeitlich nicht aktiv: das Dreifache des üblichen
/// Abstands (Median), mindestens eine Viertelstunde. Bei weniger als drei Punkten gibt es keine Lücke.
fn gap_limit_ms(points: &[HistoryPoint]) -> i64 {
    if points.len() < 3 {
        return i64::MAX;
    }
    let mut deltas: Vec<i64> = points
        .windows(2)
        .map(|w| (w[1].time - w[0].time).num_milliseconds())
        .collect();
    deltas.sort_unstable();
    (deltas[deltas.len() / 2].saturating_mul(3)).max(MIN_GAP_MS)
}

/// Teilt die Messreihe an den Lücken in zusammenhängende Stücke (Indexbereiche).
fn segments(points: &[HistoryPoint]) -> Vec<Span<usize>> {
    let limit = gap_limit_ms(points);
    let mut parts = Vec::new();
    let mut start = 0;
    for i in 1..points.len() {
        if (points[i].time - points[i - 1].time).num_milliseconds() > limit {
            parts.push(start..i);
            start = i;
        }
    }
    if !points.is_empty() {
        parts.push(start..points.len());
    }
    parts
}

/// Hochachsen-Position eines Prozentwerts (für waagerechte Schwellenlinien außerhalb von `build`).
pub fn y_for(pct: f64) -> f64 {
    let plot_h = HEIGHT - TOP - BOTTOM;
    TOP + plot_h * (1.0 - pct.clamp(0.0, 100.0) / 100.0)
}

/// Baut das Diagramm. `format_time` formatiert einen Zeitpunkt (Millisekunden seit 1970) für die Beschriftung.
pub fn build(
    points: &[HistoryPoint],
    forecast: Option<Forecast>,
    critical_pct: f32,
    range: Range,
    format_time: impl Fn(i64) -> String,
) -> ChartModel {
    let plot_w = WIDTH - LEFT - RIGHT;
    let plot_h = HEIGHT - TOP - BOTTOM;
    let y_of = |pct: f64| TOP + plot_h * (1.0 - pct.clamp(0.0, 100.0) / 100.0);

    let y_ticks = [0.0, 25.0, 50.0, 75.0, 100.0]
        .into_iter()
        .map(|p| Tick {
            position: y_of(p),
            label: format!("{p:.0} %"),
        })
        .collect();
    let mut model = ChartModel {
        width: WIDTH,
        height: HEIGHT,
        left: LEFT,
        right: WIDTH - RIGHT,
        top: TOP,
        bottom: HEIGHT - BOTTOM,
        soc_path: String::new(),
        area_path: String::new(),
        dots: Vec::new(),
        forecast_path: None,
        band_path: None,
        forecast_band_path: None,
        threshold_y: y_of(f64::from(critical_pct)),
        y_ticks,
        x_ticks: Vec::new(),
        points: Vec::new(),
    };
    if points.is_empty() {
        return model;
    }

    let to_ms = |p: &HistoryPoint| p.time.timestamp_millis() as f64;
    let first = to_ms(&points[0]);
    let last_point = &points[points.len() - 1];
    let last = to_ms(last_point);
    let last_soc = f64::from(last_point.soc_percent);

    // Prognose: höchstens ein Viertel des Zeitraums über die letzte Messung hinaus.
    let data_hours = (last - first) / 3_600_000.0;
    let horizon_hours = forecast.map_or(0.0, |f| {
        f64::from(f.hours_to_empty).min(range_hours(range, data_hours) * 0.25)
    });
    let end = (last + horizon_hours * 3_600_000.0).max(first + 60_000.0);
    let x_of = |ms: f64| LEFT + plot_w * ((ms - first) / (end - first));

    model.points = points
        .iter()
        .map(|p| ChartPoint {
            x: x_of(to_ms(p)),
            y: y_of(f64::from(p.soc_percent)),
            time_ms: p.time.timestamp_millis(),
            soc: p.soc_percent,
        })
        .collect();
    // Kurve, Fläche und Fehlerband je zusammenhängendem Stück; zwischen den Stücken bleibt die Lücke leer.
    let has_sigma = points.iter().any(|p| p.soc_sigma_percent.is_some());
    let edge = |p: &HistoryPoint, sign: f64| {
        let margin = f64::from(shared::estimator::margin_95(p.soc_sigma_percent.unwrap_or(0.0)));
        y_of(f64::from(p.soc_percent) + sign * margin)
    };
    let mut lines = Vec::new();
    let mut areas = Vec::new();
    let mut bands = Vec::new();
    for part in segments(points) {
        let piece = &points[part];
        if let [only] = piece {
            model.dots.push((x_of(to_ms(only)), y_of(f64::from(only.soc_percent))));
            continue;
        }
        let line: String = piece
            .iter()
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "{}{:.1} {:.1}",
                    if i == 0 { "M" } else { " L" },
                    x_of(to_ms(p)),
                    y_of(f64::from(p.soc_percent))
                )
            })
            .collect();
        areas.push(format!(
            "{line} L{:.1} {:.1} L{:.1} {:.1} Z",
            x_of(to_ms(&piece[piece.len() - 1])),
            model.bottom,
            x_of(to_ms(&piece[0])),
            model.bottom
        ));
        lines.push(line);
        // Fehlerband: obere Kante vorwärts, untere rückwärts.
        if has_sigma {
            let upper: Vec<String> = piece
                .iter()
                .map(|p| format!("{:.1} {:.1}", x_of(to_ms(p)), edge(p, 1.0)))
                .collect();
            let lower: Vec<String> = piece
                .iter()
                .rev()
                .map(|p| format!("{:.1} {:.1}", x_of(to_ms(p)), edge(p, -1.0)))
                .collect();
            bands.push(format!("M{} L{} Z", upper.join(" L"), lower.join(" L")));
        }
    }
    model.soc_path = lines.join(" ");
    model.area_path = areas.join(" ");
    if !bands.is_empty() {
        model.band_path = Some(bands.join(" "));
    }

    if let Some(f) = forecast.filter(|_| horizon_hours > 0.0) {
        let end_soc = last_soc - f64::from(f.rate_pct_per_hour) * horizon_hours;
        model.forecast_path = Some(format!(
            "M{:.1} {:.1} L{:.1} {:.1}",
            x_of(last),
            y_of(last_soc),
            x_of(end),
            y_of(end_soc)
        ));
        // Trichter: Wer früher leer ist (untere Grenze der Laufzeit), fällt steiler, wer später leer ist, flacher.
        if let (Some(low), Some(high)) = (f.hours_low, f.hours_high)
            && low > 0.0
            && high > 0.0
        {
            let fast = last_soc - last_soc / f64::from(low) * horizon_hours;
            let slow = last_soc - last_soc / f64::from(high) * horizon_hours;
            model.forecast_band_path = Some(format!(
                "M{:.1} {:.1} L{:.1} {:.1} L{:.1} {:.1} Z",
                x_of(last),
                y_of(last_soc),
                x_of(end),
                y_of(slow),
                x_of(end),
                y_of(fast)
            ));
        }
    }

    model.x_ticks = (0..5)
        .map(|i| {
            let ms = first + (end - first) * f64::from(i) / 4.0;
            Tick {
                position: x_of(ms),
                label: format_time(ms as i64),
            }
        })
        .collect();
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};

    fn series(count: i64, start: f32, per_step: f32) -> Vec<HistoryPoint> {
        let t0 = DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        (0..count)
            .map(|i| HistoryPoint {
                time: t0 + Duration::hours(i),
                soc_percent: start - per_step * i as f32,
                soc_sigma_percent: None,
                voltage_mv: 12_000.0,
                current_ma: -300.0,
                temperature_c: Some(15.0),
            })
            .collect()
    }

    fn label(ms: i64) -> String {
        format!("t{}", ms / 3_600_000 % 1000)
    }

    #[test]
    fn the_area_closes_the_line_down_to_the_baseline() {
        let model = build(&series(24, 100.0, 4.0), None, 20.0, Range::Day, label);
        assert!(model.area_path.starts_with(&model.soc_path));
        assert!(model.area_path.ends_with(" Z"));
        assert!(model.area_path.contains(&format!("{:.1}", model.bottom)));
        assert!(build(&[], None, 20.0, Range::Day, label).area_path.is_empty());
    }

    #[test]
    fn y_for_matches_the_threshold_of_the_model() {
        let model = build(&series(3, 80.0, 1.0), None, 20.0, Range::Day, label);
        assert!((y_for(20.0) - model.threshold_y).abs() < 1e-9);
        assert!(y_for(40.0) < y_for(20.0), "höhere Werte liegen weiter oben");
        assert_eq!(y_for(150.0), y_for(100.0));
        assert_eq!(y_for(-5.0), y_for(0.0));
    }

    #[test]
    fn empty_history_gives_an_empty_chart_with_axes() {
        let model = build(&[], None, 20.0, Range::Day, label);
        assert!(model.is_empty());
        assert_eq!(model.y_ticks.len(), 5);
        assert!(model.x_ticks.is_empty());
        assert!(model.forecast_path.is_none());
    }

    #[test]
    fn line_stays_inside_the_plot_area() {
        let model = build(&series(24, 100.0, 4.0), None, 20.0, Range::Day, label);
        let numbers: Vec<f64> = model
            .soc_path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert_eq!(numbers.len(), 48);
        for pair in numbers.chunks(2) {
            assert!(
                pair[0] >= model.left - 0.1 && pair[0] <= model.right + 0.1,
                "x {}",
                pair[0]
            );
            assert!(
                pair[1] >= model.top - 0.1 && pair[1] <= model.bottom + 0.1,
                "y {}",
                pair[1]
            );
        }
        assert!(model.soc_path.starts_with('M'));
    }

    #[test]
    fn higher_charge_is_higher_on_screen() {
        let model = build(&series(2, 80.0, 40.0), None, 20.0, Range::Day, label);
        let y: Vec<f64> = model
            .soc_path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .skip(1)
            .step_by(2)
            .map(|n| n.parse().unwrap())
            .collect();
        assert!(y[0] < y[1], "80 Prozent liegt über 40 Prozent");
    }

    #[test]
    fn threshold_line_sits_between_top_and_bottom_at_the_right_height() {
        let model = build(&series(3, 90.0, 1.0), None, 20.0, Range::Day, label);
        let plot_h = model.bottom - model.top;
        assert!((model.threshold_y - (model.top + plot_h * 0.8)).abs() < 1e-9);
        let zero = build(&series(3, 90.0, 1.0), None, 0.0, Range::Day, label);
        assert!((zero.threshold_y - zero.bottom).abs() < 1e-9);
    }

    #[test]
    fn forecast_extends_the_time_axis_and_points_downwards() {
        let forecast = Forecast {
            rate_pct_per_hour: 2.0,
            hours_to_empty: 30.0,
            hours_low: None,
            hours_high: None,
        };
        let with = build(&series(12, 80.0, 2.0), Some(forecast), 20.0, Range::Day, label);
        let without = build(&series(12, 80.0, 2.0), None, 20.0, Range::Day, label);
        assert!(with.forecast_path.is_some());
        assert!(without.forecast_path.is_none());
        // Mit Prognose braucht die Messkurve weniger Breite.
        let last_x = |m: &ChartModel| -> f64 {
            m.soc_path
                .rsplit(' ')
                .nth(1)
                .unwrap()
                .trim_start_matches('L')
                .parse()
                .unwrap()
        };
        assert!(last_x(&with) < last_x(&without));
        let path = with.forecast_path.unwrap();
        let nums: Vec<f64> = path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        assert!(nums[2] > nums[0], "Prognose läuft nach rechts");
        assert!(nums[3] > nums[1], "und nach unten");
    }

    #[test]
    fn forecast_horizon_is_capped_to_a_quarter_of_the_range() {
        let forecast = Forecast {
            rate_pct_per_hour: 0.01,
            hours_to_empty: 9_000.0,
            hours_low: None,
            hours_high: None,
        };
        let model = build(&series(24, 80.0, 0.01), Some(forecast), 20.0, Range::Day, label);
        let path = model.forecast_path.unwrap();
        let end_x: f64 = path
            .rsplit(' ')
            .nth(1)
            .unwrap()
            .trim_start_matches('L')
            .parse()
            .unwrap();
        assert!(end_x <= model.right + 0.1);
    }

    #[test]
    fn the_whole_history_caps_the_forecast_by_the_length_of_the_recording() {
        let forecast = Forecast {
            rate_pct_per_hour: 0.01,
            hours_to_empty: 9_000.0,
            hours_low: None,
            hours_high: None,
        };
        let share_of_data = |range| {
            let model = build(&series(241, 80.0, 0.01), Some(forecast), 20.0, range, label);
            (model.points.last().unwrap().x - model.left) / (model.right - model.left)
        };
        // 240 Stunden Aufzeichnung, Prognose höchstens ein Viertel davon: Die Messkurve füllt vier Fünftel der Breite.
        assert!((share_of_data(Range::All) - 0.8).abs() < 0.001);
        // Bei 24 Stunden Zeitraum bleibt es bei 6 Stunden Prognose, die Kurve füllt fast alles.
        assert!(share_of_data(Range::Day) > 0.95);
    }

    #[test]
    fn points_carry_screen_positions_and_nearest_finds_the_closest() {
        let model = build(&series(5, 80.0, 10.0), None, 20.0, Range::Day, label);
        assert_eq!(model.points.len(), 5);
        assert_eq!(model.points[0].soc, 80.0);
        assert_eq!(nearest(&model.points, model.points[3].x + 1.0), Some(3));
        assert_eq!(nearest(&model.points, -100.0), Some(0));
        assert_eq!(nearest(&model.points, 10_000.0), Some(4));
        assert_eq!(nearest(&[], 5.0), None);
        assert!(build(&[], None, 20.0, Range::Day, label).points.is_empty());
    }

    #[test]
    fn five_time_labels_from_the_formatter() {
        let model = build(&series(5, 80.0, 1.0), None, 20.0, Range::Day, label);
        assert_eq!(model.x_ticks.len(), 5);
        assert!(model.x_ticks[0].position < model.x_ticks[4].position);
        assert_eq!(
            model.x_ticks[0].label,
            "t0".replace(
                "t0",
                &label(
                    DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
                        .unwrap()
                        .timestamp_millis()
                )
            )
        );
    }

    #[test]
    fn single_point_does_not_divide_by_zero() {
        let model = build(&series(1, 80.0, 0.0), None, 20.0, Range::Day, label);
        assert!(!model.soc_path.contains("NaN") && !model.soc_path.contains("inf"));
        assert!(!model.is_empty(), "ein einzelner Messpunkt ist ein Messpunkt");
        assert_eq!(model.dots.len(), 1);
        assert!(model.dots[0].0.is_finite() && model.dots[0].1.is_finite());
    }

    /// Messreihe mit Messpunkten zu den angegebenen Stunden seit Beginn.
    fn at_hours(hours: &[i64]) -> Vec<HistoryPoint> {
        let base = series(1, 80.0, 0.0);
        hours
            .iter()
            .map(|h| HistoryPoint {
                time: base[0].time + Duration::hours(*h),
                ..base[0].clone()
            })
            .collect()
    }

    #[test]
    fn a_pause_in_the_readings_breaks_the_line_the_area_and_the_band() {
        // Stündliche Messung, dann acht Stunden nichts, dann wieder stündlich.
        let mut points = at_hours(&[0, 1, 2, 3, 4, 12, 13, 14, 15, 16]);
        for p in &mut points {
            p.soc_sigma_percent = Some(1.0);
        }
        let model = build(&points, None, 20.0, Range::Week, label);
        assert_eq!(
            model.soc_path.matches('M').count(),
            2,
            "zwei Teilstücke: {}",
            model.soc_path
        );
        assert_eq!(model.area_path.matches('M').count(), 2);
        assert_eq!(model.area_path.matches('Z').count(), 2);
        assert_eq!(model.band_path.as_ref().unwrap().matches('M').count(), 2);
        assert!(model.dots.is_empty());
        // Die Lücke selbst bleibt leer: Der Sprung von Stunde 4 zu Stunde 12 liegt in keinem Pfad.
        let x_of_hour = |h: usize| model.points[h].x;
        let numbers: Vec<f64> = model
            .soc_path
            .replace(['M', 'L'], " ")
            .split_whitespace()
            .map(|n| n.parse().unwrap())
            .collect();
        let xs: Vec<f64> = numbers.chunks(2).map(|c| c[0]).collect();
        assert_eq!(xs.len(), 10);
        assert!((xs[4] - x_of_hour(4)).abs() < 0.06 && (xs[5] - x_of_hour(5)).abs() < 0.06);
        assert!(model.soc_path.contains(&format!(" M{:.1} ", x_of_hour(5))));
    }

    #[test]
    fn an_imported_day_and_a_new_start_twelve_days_later_are_two_pieces() {
        // So lag TN0001 auf Prod: ein Tag stündlich eingelesen, dann zwölf Tage nichts, dann wieder Messwerte.
        let mut hours: Vec<i64> = (0..24).collect();
        hours.extend([24 * 13, 24 * 13 + 1, 24 * 13 + 2]);
        let model = build(&at_hours(&hours), None, 20.0, Range::All, label);
        assert_eq!(model.soc_path.matches('M').count(), 2, "{}", model.soc_path);
        assert_eq!(model.area_path.matches('Z').count(), 2);
        // Das zweite Stück beginnt mit eigenem "M": Über die Lücke zwischen Stunde 23 und Stunde 312 läuft keine Linie.
        let gap_start = model.points[23].x;
        let gap_end = model.points[24].x;
        assert!(gap_end - gap_start > 100.0, "die Lücke ist breit");
        assert!(model.soc_path.contains(&format!(" M{gap_end:.1} ")));
    }

    #[test]
    fn evenly_spaced_readings_stay_one_line_even_when_far_apart() {
        // Alle zwölf Stunden eine Messung ist der Normalfall dieser Reihe, keine Lücke.
        let model = build(&at_hours(&[0, 12, 24, 36, 48]), None, 20.0, Range::Week, label);
        assert_eq!(model.soc_path.matches('M').count(), 1);
        assert!(model.dots.is_empty());
    }

    #[test]
    fn a_short_stop_is_no_gap_but_a_long_one_is() {
        // Üblicher Abstand eine Stunde: Dreifaches ist die Grenze. Zwei Stunden Pause bleiben Linie, vier brechen ab.
        let short = build(&at_hours(&[0, 1, 2, 4, 5, 6]), None, 20.0, Range::Week, label);
        assert_eq!(short.soc_path.matches('M').count(), 1);
        let long = build(&at_hours(&[0, 1, 2, 6, 7, 8]), None, 20.0, Range::Week, label);
        assert_eq!(long.soc_path.matches('M').count(), 2);
    }

    #[test]
    fn a_lone_reading_between_two_gaps_becomes_a_dot() {
        let model = build(
            &at_hours(&[0, 1, 2, 3, 20, 40, 41, 42, 43]),
            None,
            20.0,
            Range::Month,
            label,
        );
        assert_eq!(model.soc_path.matches('M').count(), 2);
        assert_eq!(model.dots.len(), 1);
        assert!((model.dots[0].0 - model.points[4].x).abs() < 0.06);
        assert!((model.dots[0].1 - model.points[4].y).abs() < 0.06);
    }

    #[test]
    fn fewer_than_three_readings_never_count_as_a_gap() {
        let model = build(&at_hours(&[0, 48]), None, 20.0, Range::Month, label);
        assert_eq!(model.soc_path.matches('M').count(), 1);
        assert!(model.dots.is_empty());
    }

    #[test]
    fn dense_data_needs_a_pause_of_at_least_a_quarter_of_an_hour() {
        // Alle zwei Minuten eine Messung: Dreifaches wären sechs Minuten, die Grenze liegt bei 15 Minuten.
        let base = series(1, 80.0, 0.0);
        let at_minutes = |minutes: &[i64]| -> Vec<HistoryPoint> {
            minutes
                .iter()
                .map(|m| HistoryPoint {
                    time: base[0].time + Duration::minutes(*m),
                    ..base[0].clone()
                })
                .collect()
        };
        let calm = build(&at_minutes(&[0, 2, 4, 6, 8, 20, 22, 24]), None, 20.0, Range::Day, label);
        assert_eq!(
            calm.soc_path.matches('M').count(),
            1,
            "12 Minuten Pause sind keine Lücke"
        );
        let gap = build(&at_minutes(&[0, 2, 4, 6, 8, 30, 32, 34]), None, 20.0, Range::Day, label);
        assert_eq!(gap.soc_path.matches('M').count(), 2, "22 Minuten Pause sind eine Lücke");
    }

    #[test]
    fn a_custom_range_is_scaled_like_the_whole_history() {
        let forecast = Forecast {
            rate_pct_per_hour: 0.01,
            hours_to_empty: 9_000.0,
            hours_low: None,
            hours_high: None,
        };
        let share = |range| {
            let model = build(&series(241, 80.0, 0.01), Some(forecast), 20.0, range, label);
            (model.points.last().unwrap().x - model.left) / (model.right - model.left)
        };
        assert!((share(Range::Custom) - share(Range::All)).abs() < 1e-9);
    }
}
