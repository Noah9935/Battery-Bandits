//! Marker für die Standortkarte als JSON für die JavaScript-Brücke (rein, ohne Browser).

use serde::Serialize;
use shared::{DataSource, Device, DeviceStatus};

use crate::format;

#[derive(Serialize)]
struct Marker {
    id: String,
    label: String,
    status: DeviceStatus,
    source: DataSource,
    /// Kritisches Gerät, das pulsieren soll.
    pulse: bool,
    lat: f64,
    lon: f64,
}

/// Marker für alle Geräte mit bekannter Position als JSON für die Karte.
pub fn markers_json(devices: &[Device], pulse: bool) -> String {
    let markers: Vec<Marker> = devices
        .iter()
        .filter_map(|d| {
            Some(Marker {
                id: d.id.clone(),
                label: format!(
                    "{}: {}",
                    format::device_title(&d.id, d.name.as_deref()),
                    d.soc_percent
                        .map_or_else(|| "Ladestand unbekannt".to_owned(), format::percent)
                ),
                status: d.status,
                source: d.data_source,
                pulse: pulse && d.status == DeviceStatus::Critical,
                lat: d.lat?,
                lon: d.lon?,
            })
        })
        .collect();
    serde_json::to_string(&markers).unwrap_or_else(|_| "[]".to_owned())
}

/// Hinweis zu Geräten ohne Position, mit richtigem Singular.
pub fn missing_fix_text(count: usize) -> String {
    if count == 1 {
        "1 Gerät ohne GPS-Fix ist nicht auf der Karte.".to_owned()
    } else {
        format!("{count} Geräte ohne GPS-Fix sind nicht auf der Karte.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn device(id: &str, position: Option<(f64, f64)>) -> Device {
        Device {
            id: id.into(),
            name: Some("Hauptbahnhof".into()),
            status: DeviceStatus::Critical,
            soc_percent: Some(12.34),
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: position.map(|p| p.0),
            lon: position.map(|p| p.1),
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Simulation,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn only_devices_with_a_position_become_markers() {
        let json = markers_json(&[device("TN0001", Some((48.78, 9.18))), device("TN0002", None)], true);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let list = value.as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], "TN0001");
        assert_eq!(list[0]["status"], "critical");
        assert_eq!(list[0]["source"], "simulation");
        assert_eq!(list[0]["pulse"], true);
        assert_eq!(list[0]["lat"], 48.78);
        assert_eq!(list[0]["label"], "Hauptbahnhof (TN0001): 12,3 %");
    }

    #[test]
    fn missing_fix_text_has_correct_grammar() {
        assert_eq!(missing_fix_text(1), "1 Gerät ohne GPS-Fix ist nicht auf der Karte.");
        assert_eq!(missing_fix_text(3), "3 Geräte ohne GPS-Fix sind nicht auf der Karte.");
    }

    #[test]
    fn no_devices_give_an_empty_list() {
        assert_eq!(markers_json(&[], true), "[]");
    }
}
