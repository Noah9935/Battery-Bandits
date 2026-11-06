//! Eingaben des Schwellwert-Formulars: Zahlen lesen und Einstellungen zusammensetzen (rein, ohne Browser).

use shared::Settings;

/// Zahl aus einem Eingabefeld, Komma oder Punkt als Dezimalzeichen.
pub fn parse_number(text: &str) -> Option<f32> {
    let value: f32 = text.trim().replace(',', ".").parse().ok()?;
    value.is_finite().then_some(value)
}

/// Neue Einstellungen aus den Eingaben, Offline-Zeit in Minuten. Alle übrigen Werte bleiben erhalten.
pub fn apply_inputs(
    current: &Settings,
    critical: &str,
    warning: &str,
    offline_minutes: &str,
) -> Result<Settings, String> {
    let critical = parse_number(critical).ok_or("Der kritische Schwellwert ist keine Zahl.")?;
    let warning = parse_number(warning).ok_or("Der Warnschwellwert ist keine Zahl.")?;
    let minutes = parse_number(offline_minutes).ok_or("Die Offline-Zeit ist keine Zahl.")?;
    let seconds = (minutes * 60.0).round();
    if !(0.0..=f32::from(u16::MAX) * 60.0).contains(&seconds) {
        return Err("Die Offline-Zeit ist zu groß.".to_owned());
    }
    Ok(Settings {
        threshold_critical_pct: critical,
        threshold_warning_pct: warning,
        offline_timeout_seconds: seconds as i32,
        ..current.clone()
    })
}

pub fn number_text(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}").replace('.', ",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comma_and_point() {
        assert_eq!(parse_number("12,5"), Some(12.5));
        assert_eq!(parse_number(" 20 "), Some(20.0));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(parse_number("inf"), None);
    }

    #[test]
    fn inputs_replace_only_the_three_values() {
        let current = Settings::default();
        let new = apply_inputs(&current, "25", "50,5", "30").unwrap();
        assert_eq!(new.threshold_critical_pct, 25.0);
        assert_eq!(new.threshold_warning_pct, 50.5);
        assert_eq!(new.offline_timeout_seconds, 1800);
        assert_eq!(new.min_voltage_mv, current.min_voltage_mv);
        assert_eq!(new.battery_capacity_ah, current.battery_capacity_ah);
    }

    #[test]
    fn bad_inputs_give_german_messages() {
        let current = Settings::default();
        assert!(
            apply_inputs(&current, "x", "40", "15")
                .unwrap_err()
                .contains("kritische")
        );
        assert!(
            apply_inputs(&current, "20", "", "15")
                .unwrap_err()
                .contains("Warnschwellwert")
        );
        assert!(
            apply_inputs(&current, "20", "40", "-3")
                .unwrap_err()
                .contains("zu groß")
        );
    }

    #[test]
    fn number_text_is_short_and_german() {
        assert_eq!(number_text(20.0), "20");
        assert_eq!(number_text(12.5), "12,5");
        assert_eq!(number_text(15.0), "15");
    }
}
