//! Geometrie des Verlaufsdiagramms (SVG), rein und testbar.
//!
//! Die Zeitachse läuft vom ersten Messpunkt bis zum letzten, bei vorhandener Prognose zusätzlich ein
//! Stück in die Zukunft. Die Hochachse ist fest 0 bis 100 Prozent. Die gestrichelte waagerechte Linie
//! zeigt den kritischen Schwellwert.

use shared::Forecast;
use shared::api::{HistoryPoint, Range};

pub const WIDTH: f64 = 640.0;
pub const HEIGHT: f64 = 260.0;
const LEFT: f64 = 44.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const BOTTOM: f64 = 30.0;

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
    /// Pfad der Ladezustandskurve, leer ohne Messpunkte.
    pub soc_path: String,
    /// Gestrichelte Prognose vom letzten Punkt bis zum Ende des Zeitfensters.
    pub forecast_path: Option<String>,
    /// Hochachsen-Position des kritischen Schwellwerts.
    pub threshold_y: f64,
    pub y_ticks: Vec<Tick>,
    pub x_ticks: Vec<Tick>,
    /// Alle Messpunkte mit Position, für das Fadenkreuz.
    pub points: Vec<ChartPoint>,
}

impl ChartModel {
    pub fn is_empty(&self) -> bool {
        self.soc_path.is_empty()
    }
}

fn range_hours(range: Range) -> f64 {
    match range {
        Range::Day => 24.0,
        Range::Week => 7.0 * 24.0,
        Range::Month => 30.0 * 24.0,
    }
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
        forecast_path: None,
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
    let horizon_hours = forecast.map_or(0.0, |f| f64::from(f.hours_to_empty).min(range_hours(range) * 0.25));
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
    model.soc_path = points
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

    if let Some(f) = forecast.filter(|_| horizon_hours > 0.0) {
        let end_soc = last_soc - f64::from(f.rate_pct_per_hour) * horizon_hours;
        model.forecast_path = Some(format!(
            "M{:.1} {:.1} L{:.1} {:.1}",
            x_of(last),
            y_of(last_soc),
            x_of(end),
            y_of(end_soc)
        ));
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
    }
}
