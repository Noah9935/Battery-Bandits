//! Standortkarte: Filter der Geräte und Marker als JSON für die JavaScript-Brücke (rein, ohne Browser).

use std::collections::BTreeSet;

use serde::Serialize;
use shared::{DataSource, Device, DeviceStatus};

use crate::format;

/// Filter der Kartenseite. Vorgabe: alles zeigen.
#[derive(Debug, Clone, PartialEq)]
pub struct MapFilter {
    pub search: String,
    pub statuses: BTreeSet<DeviceStatus>,
    pub sources: BTreeSet<DataSource>,
    /// Nur Geräte bis zu diesem Ladestand in Prozent (100 heißt: kein Limit). Geräte ohne Ladezustand zählen bei 100 mit.
    pub max_soc: f32,
    /// Nur die Nodes zeigen, die man zum Wechseln ausgewählt hat (die Merkliste).
    pub only_selected: bool,
}

impl Default for MapFilter {
    fn default() -> Self {
        Self {
            search: String::new(),
            statuses: DeviceStatus::ALL.iter().copied().collect(),
            sources: DataSource::ALL.iter().copied().collect(),
            max_soc: 100.0,
            only_selected: false,
        }
    }
}

impl MapFilter {
    /// Ist etwas eingeschränkt?
    pub fn is_active(&self) -> bool {
        *self != Self::default()
    }
}

/// Geräte, die der Filter durchlässt (mit und ohne Position): erst die vom Dozenten, sonst in der Reihenfolge der Eingabe.
/// `selected` sind die ausgewählten Nodes, sie zählen nur bei `only_selected`.
pub fn visible<'a>(devices: &'a [Device], filter: &MapFilter, selected: &BTreeSet<String>) -> Vec<&'a Device> {
    let needle = filter.search.trim().to_lowercase();
    let mut list: Vec<&Device> = devices
        .iter()
        .filter(|d| !filter.only_selected || selected.contains(&d.id))
        .filter(|d| filter.statuses.contains(&d.status))
        .filter(|d| filter.sources.contains(&d.data_source))
        .filter(|d| match d.soc_percent {
            Some(soc) => soc <= filter.max_soc + f32::EPSILON,
            None => filter.max_soc >= 100.0,
        })
        .filter(|d| {
            needle.is_empty()
                || d.id.to_lowercase().contains(&needle)
                || d.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&needle))
        })
        .collect();
    // Geräte vom Dozenten stehen vor den erfundenen (die Reihenfolge darin bleibt wie geliefert).
    list.sort_by_key(|d| d.data_source.rank());
    list
}

/// Auswahl im Filtermenü: Ist noch nichts eingeschränkt (alles an), schränkt ein Tipp auf einen Eintrag auf genau
/// diesen ein. Danach schaltet ein Tipp den Eintrag an oder aus. Bleibt nichts übrig, ist wieder alles an.
pub fn narrow_toggle<T: Ord + Copy>(set: &mut BTreeSet<T>, all: &[T], item: T) {
    if set.len() == all.len() {
        set.clear();
        set.insert(item);
    } else if !set.remove(&item) {
        set.insert(item);
    }
    if set.is_empty() {
        set.extend(all.iter().copied());
    }
}

/// Ist der Eintrag im Filtermenü angehakt? Nur wenn eingeschränkt ist und er dazugehört.
pub fn narrowed_has<T: Ord>(set: &BTreeSet<T>, all: &[T], item: &T) -> bool {
    set.len() < all.len() && set.contains(item)
}

/// Kurzer Text für den Filterknopf: `Alle` oder zum Beispiel `Kritisch, Dozent, Merkliste`.
pub fn filter_summary(filter: &MapFilter) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if filter.statuses.len() < DeviceStatus::ALL.len() {
        parts.extend(
            DeviceStatus::ALL
                .iter()
                .filter(|s| filter.statuses.contains(s))
                .map(|s| format::status_label(*s)),
        );
    }
    if filter.sources.len() < DataSource::ALL.len() {
        parts.extend(
            DataSource::ALL
                .iter()
                .filter(|s| filter.sources.contains(s))
                .map(|s| format::source_label(*s)),
        );
    }
    if filter.only_selected {
        parts.push("Merkliste");
    }
    if parts.is_empty() {
        "Alle".to_owned()
    } else {
        parts.join(", ")
    }
}

#[derive(Serialize)]
struct Marker {
    id: String,
    label: String,
    status: DeviceStatus,
    source: DataSource,
    lat: f64,
    lon: f64,
}

/// Marker für alle übergebenen Geräte mit bekannter Position als JSON für die Karte.
pub fn markers_json(devices: &[&Device]) -> String {
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

    fn device(
        id: &str,
        status: DeviceStatus,
        source: DataSource,
        soc: Option<f32>,
        position: Option<(f64, f64)>,
    ) -> Device {
        Device {
            id: id.into(),
            name: Some("Hauptbahnhof".into()),
            status,
            soc_percent: soc,
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
            data_source: source,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    fn fleet() -> Vec<Device> {
        vec![
            device(
                "TN0001",
                DeviceStatus::Normal,
                DataSource::Dozent,
                Some(90.0),
                Some((48.7, 9.1)),
            ),
            device(
                "SIM01",
                DeviceStatus::Critical,
                DataSource::Simulation,
                Some(10.0),
                Some((48.8, 9.2)),
            ),
            device("SIM02", DeviceStatus::Warning, DataSource::Simulation, Some(35.0), None),
            device("101", DeviceStatus::Offline, DataSource::Dozent, None, None),
        ]
    }

    fn ids(list: &[&Device]) -> Vec<String> {
        list.iter().map(|d| d.id.clone()).collect()
    }

    #[test]
    fn the_filter_menu_narrows_down_from_everything() {
        let all = DeviceStatus::ALL;
        let mut set: BTreeSet<DeviceStatus> = all.iter().copied().collect();
        assert!(!narrowed_has(&set, all, &DeviceStatus::Critical));
        narrow_toggle(&mut set, all, DeviceStatus::Critical);
        assert_eq!(set.len(), 1);
        assert!(narrowed_has(&set, all, &DeviceStatus::Critical));
        narrow_toggle(&mut set, all, DeviceStatus::Offline);
        assert_eq!(set.len(), 2);
        narrow_toggle(&mut set, all, DeviceStatus::Offline);
        narrow_toggle(&mut set, all, DeviceStatus::Critical);
        // Nichts übrig: wieder alles an.
        assert_eq!(set.len(), all.len());
    }

    #[test]
    fn the_filter_button_tells_what_is_narrowed() {
        let mut f = MapFilter::default();
        assert_eq!(filter_summary(&f), "Alle");
        f.statuses = [DeviceStatus::Critical].into();
        f.sources = [DataSource::Dozent].into();
        f.only_selected = true;
        assert_eq!(filter_summary(&f), "Kritisch, Dozent, Merkliste");
    }

    #[test]
    fn default_filter_shows_everything() {
        let devices = fleet();
        assert_eq!(visible(&devices, &MapFilter::default(), &BTreeSet::new()).len(), 4);
        assert!(!MapFilter::default().is_active());
    }

    #[test]
    fn filters_by_status_and_source() {
        let devices = fleet();
        let mut f = MapFilter::default();
        f.statuses.remove(&DeviceStatus::Offline);
        f.statuses.remove(&DeviceStatus::Normal);
        assert_eq!(ids(&visible(&devices, &f, &BTreeSet::new())), ["SIM01", "SIM02"]);
        f.sources.remove(&DataSource::Simulation);
        assert!(visible(&devices, &f, &BTreeSet::new()).is_empty());
        assert!(f.is_active());
    }

    #[test]
    fn max_soc_hides_fuller_devices_but_keeps_unknown_only_without_limit() {
        let devices = fleet();
        let limited = MapFilter {
            max_soc: 40.0,
            ..MapFilter::default()
        };
        assert_eq!(ids(&visible(&devices, &limited, &BTreeSet::new())), ["SIM01", "SIM02"]);
        assert_eq!(visible(&devices, &MapFilter::default(), &BTreeSet::new()).len(), 4);
    }

    #[test]
    fn search_matches_id_and_name() {
        let devices = fleet();
        let f = MapFilter {
            search: " sim0".into(),
            ..MapFilter::default()
        };
        assert_eq!(ids(&visible(&devices, &f, &BTreeSet::new())), ["SIM01", "SIM02"]);
        let by_name = MapFilter {
            search: "bahnhof".into(),
            ..MapFilter::default()
        };
        assert_eq!(visible(&devices, &by_name, &BTreeSet::new()).len(), 4);
        let none = MapFilter {
            search: "gibtsnicht".into(),
            ..MapFilter::default()
        };
        assert!(visible(&devices, &none, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn only_devices_with_a_position_become_markers() {
        let devices = fleet();
        let all: Vec<&Device> = devices.iter().collect();
        let value: serde_json::Value = serde_json::from_str(&markers_json(&all)).unwrap();
        let list = value.as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1]["id"], "SIM01");
        assert_eq!(list[1]["status"], "critical");
        assert_eq!(list[1]["source"], "simulation");
        assert_eq!(list[1]["lat"], 48.8);
        assert_eq!(list[1]["label"], "Hauptbahnhof (SIM01): 10,0 %");
        assert!(list[0].get("pulse").is_none(), "keine Pulsieren-Angabe mehr");
    }

    #[test]
    fn missing_fix_text_has_correct_grammar() {
        assert_eq!(missing_fix_text(1), "1 Gerät ohne GPS-Fix ist nicht auf der Karte.");
        assert_eq!(missing_fix_text(3), "3 Geräte ohne GPS-Fix sind nicht auf der Karte.");
    }

    #[test]
    fn no_devices_give_an_empty_list() {
        assert_eq!(markers_json(&[]), "[]");
    }

    #[test]
    fn only_selected_shows_just_the_chosen_nodes() {
        let devices = fleet();
        let chosen: BTreeSet<String> = ["SIM01".to_owned(), "101".to_owned()].into();
        let f = MapFilter {
            only_selected: true,
            ..MapFilter::default()
        };
        assert!(f.is_active());
        assert_eq!(ids(&visible(&devices, &f, &chosen)), ["101", "SIM01"], "Dozent zuerst");
        assert!(visible(&devices, &f, &BTreeSet::new()).is_empty());
        // Ohne den Schalter ist die Auswahl egal.
        assert_eq!(visible(&devices, &MapFilter::default(), &chosen).len(), 4);
    }
}
