//! CSV-Export eines Messwertverlaufs (rein).
//!
//! Format: Komma als Trennzeichen, Punkt als Dezimalzeichen, Zeitangaben in UTC nach ISO 8601.
//! Fehlende Werte bleiben leer.

use shared::Reading;

pub const HEADER: &str = "time,device_id,voltage_mv,current_ma,temperature_c,soc_percent,lat,lon,altitude_m,satellites";

/// Quotet ein Feld, wenn es Trennzeichen, Anführungszeichen oder Zeilenumbrüche enthält.
pub fn field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn opt<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

pub fn readings_to_csv(readings: &[Reading]) -> String {
    let mut out = String::with_capacity(HEADER.len() + readings.len() * 90);
    out.push_str(HEADER);
    out.push_str("\r\n");
    for r in readings {
        let row = [
            r.time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            field(&r.device_id),
            r.voltage_mv.to_string(),
            r.current_ma.to_string(),
            r.temperature_c.to_string(),
            format!("{:.2}", r.soc_percent),
            opt(r.lat),
            opt(r.lon),
            opt(r.altitude_m),
            opt(r.satellites),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

/// Dateiname für den Download: nur Buchstaben, Ziffern, Minus und Unterstrich.
pub fn file_name(device_id: &str, range: &str) -> String {
    let safe: String = device_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    format!("{safe}_{range}.csv")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn reading(position: Option<(f64, f64)>) -> Reading {
        Reading {
            time: DateTime::parse_from_rfc3339("2026-11-06T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc),
            device_id: "TN0001".into(),
            voltage_mv: 12_296,
            current_ma: -311,
            temperature_c: 16.5,
            soc_percent: 81.6364,
            lat: position.map(|p| p.0),
            lon: position.map(|p| p.1),
            altitude_m: None,
            satellites: position.map(|_| 7),
        }
    }

    #[test]
    fn empty_export_has_only_the_header() {
        assert_eq!(readings_to_csv(&[]), format!("{HEADER}\r\n"));
    }

    #[test]
    fn rows_follow_the_header_order() {
        let csv = readings_to_csv(&[reading(Some((48.8, 9.0)))]);
        let row = csv.lines().nth(1).unwrap();
        assert_eq!(row, "2026-11-06T10:30:00Z,TN0001,12296,-311,16.5,81.64,48.8,9,,7");
        assert_eq!(csv.lines().next().unwrap().split(',').count(), row.split(',').count());
    }

    #[test]
    fn missing_values_stay_empty() {
        let csv = readings_to_csv(&[reading(None)]);
        assert!(csv.lines().nth(1).unwrap().ends_with("81.64,,,,"));
    }

    #[test]
    fn fields_with_special_characters_are_quoted() {
        assert_eq!(field("TN,1"), "\"TN,1\"");
        assert_eq!(field("a\"b"), "\"a\"\"b\"");
        assert_eq!(field("TN0001"), "TN0001");
    }

    #[test]
    fn file_name_is_safe() {
        assert_eq!(file_name("TN0001", "24h"), "TN0001_24h.csv");
        assert_eq!(file_name("../etc/pass wd", "7d"), "___etc_pass_wd_7d.csv");
    }
}
