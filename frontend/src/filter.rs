//! Filtern und Sortieren der Geräteliste im Browser.
//!
//! Gleiche Regeln wie im Dashboard-Service (`/api/devices`), damit beide dasselbe Ergebnis liefern.

use std::cmp::Ordering;

use shared::api::{DeviceSort, SortDir};
use shared::{DataSource, Device, DeviceStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewFilter {
    pub status: Option<DeviceStatus>,
    /// Nur Geräte mit dieser Datenherkunft.
    pub source: Option<DataSource>,
    pub search: String,
    pub sort: DeviceSort,
    pub dir: SortDir,
    /// Kritische und Offline-Geräte vor allen anderen.
    pub attention_first: bool,
}

impl Default for ViewFilter {
    fn default() -> Self {
        Self {
            status: None,
            source: None,
            search: String::new(),
            sort: DeviceSort::Id,
            dir: SortDir::Asc,
            attention_first: true,
        }
    }
}

/// Anzahl Geräte je Herkunft: (vom Dozenten, erfunden).
pub fn count_by_source(devices: &[Device]) -> (usize, usize) {
    let real = devices.iter().filter(|d| d.data_source == DataSource::Dozent).count();
    (real, devices.len() - real)
}

/// So viele Geräte stehen auf einer Seite der Übersicht. Mehr Zeilen auf einmal überfordern vor allem Handys.
pub const PAGE_SIZE: usize = 20;

/// Anfang, Ende (ausschließlich) und Zahl der Seiten für die gewünschte Seite (ab 0). Eine zu große Seite wird auf die
/// letzte gekürzt, zum Beispiel wenn ein Filter die Liste verkleinert.
pub fn page_bounds(len: usize, page: usize, size: usize) -> (usize, usize, usize) {
    let size = size.max(1);
    let pages = len.div_ceil(size).max(1);
    let page = page.min(pages - 1);
    let start = page * size;
    (start, (start + size).min(len), pages)
}

/// Wie [`apply`], aber angeheftete Geräte (Favoriten) stehen zuerst. Die Reihenfolge innerhalb der Gruppen bleibt.
pub fn apply_pinned(
    devices: &[Device],
    filter: &ViewFilter,
    pinned: &std::collections::BTreeSet<String>,
) -> Vec<Device> {
    let mut result = apply(devices, filter);
    result.sort_by_key(|d| !pinned.contains(&d.id));
    result
}

fn severity(status: DeviceStatus) -> u8 {
    match status {
        DeviceStatus::Critical => 0,
        DeviceStatus::Offline => 1,
        DeviceStatus::Warning => 2,
        DeviceStatus::Normal => 3,
    }
}

/// Unbekannte Werte stehen immer hinten, unabhängig von der Richtung.
fn compare_optional<T: PartialOrd>(a: Option<T>, b: Option<T>, dir: SortDir) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let ordering = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if dir == SortDir::Desc {
                ordering.reverse()
            } else {
                ordering
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn matches_search(device: &Device, search: &str) -> bool {
    let needle = search.trim().to_lowercase();
    needle.is_empty()
        || device.id.to_lowercase().contains(&needle)
        || device
            .name
            .as_deref()
            .is_some_and(|n| n.to_lowercase().contains(&needle))
}

pub fn apply(devices: &[Device], filter: &ViewFilter) -> Vec<Device> {
    let mut result: Vec<Device> = devices
        .iter()
        .filter(|d| filter.status.is_none_or(|s| d.status == s))
        .filter(|d| filter.source.is_none_or(|s| d.data_source == s))
        .filter(|d| matches_search(d, &filter.search))
        .cloned()
        .collect();
    result.sort_by(|a, b| {
        // "Wichtigstes zuerst": Geräte vom Dozenten vor den erfundenen, darin die mit Handlungsbedarf.
        let attention = if filter.attention_first {
            a.data_source
                .rank()
                .cmp(&b.data_source.rank())
                .then((!a.status.needs_attention()).cmp(&!b.status.needs_attention()))
        } else {
            Ordering::Equal
        };
        let key = match filter.sort {
            DeviceSort::Id => {
                let ordering = a.id.cmp(&b.id);
                if filter.dir == SortDir::Desc {
                    ordering.reverse()
                } else {
                    ordering
                }
            }
            DeviceSort::SocPercent => compare_optional(a.soc_percent, b.soc_percent, filter.dir),
            DeviceSort::LastSeenAt => compare_optional(a.last_seen_at, b.last_seen_at, filter.dir),
            DeviceSort::Status => {
                let ordering = severity(a.status).cmp(&severity(b.status));
                if filter.dir == SortDir::Desc {
                    ordering.reverse()
                } else {
                    ordering
                }
            }
        };
        attention.then(key).then_with(|| a.id.cmp(&b.id))
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};

    fn at(minutes_ago: i64) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            - Duration::minutes(minutes_ago)
    }

    fn device(id: &str, name: Option<&str>, status: DeviceStatus, soc: Option<f32>, seen: Option<i64>) -> Device {
        Device {
            id: id.into(),
            name: name.map(str::to_owned),
            status,
            soc_percent: soc,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            last_seen_at: seen.map(at),
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: at(0),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    /// Dieselben Geräte wie in den Tests des Dashboard-Service.
    fn fleet() -> Vec<Device> {
        vec![
            device(
                "TN0001",
                Some("Hauptbahnhof"),
                DeviceStatus::Normal,
                Some(90.0),
                Some(0),
            ),
            device(
                "TN0002",
                Some("Schlossplatz"),
                DeviceStatus::Warning,
                Some(35.0),
                Some(5),
            ),
            device(
                "TN0003",
                Some("Killesberg"),
                DeviceStatus::Critical,
                Some(12.0),
                Some(1),
            ),
            device("TN0004", None, DeviceStatus::Offline, Some(60.0), Some(180)),
            device("TN0005", Some("Neu"), DeviceStatus::Normal, None, None),
        ]
    }

    fn ids(devices: &[Device]) -> Vec<&str> {
        devices.iter().map(|d| d.id.as_str()).collect()
    }

    fn view(f: impl FnOnce(&mut ViewFilter)) -> ViewFilter {
        let mut filter = ViewFilter {
            attention_first: false,
            ..ViewFilter::default()
        };
        f(&mut filter);
        filter
    }

    #[test]
    fn default_shows_attention_first_then_by_id() {
        let result = apply(&fleet(), &ViewFilter::default());
        assert_eq!(ids(&result), ["TN0003", "TN0004", "TN0001", "TN0002", "TN0005"]);
    }

    #[test]
    fn filters_by_status() {
        let result = apply(&fleet(), &view(|f| f.status = Some(DeviceStatus::Critical)));
        assert_eq!(ids(&result), ["TN0003"]);
    }

    #[test]
    fn filters_by_data_source() {
        let mut devices = fleet();
        devices[1].data_source = DataSource::Simulation;
        devices[4].data_source = DataSource::Simulation;
        let invented = apply(&devices, &view(|f| f.source = Some(DataSource::Simulation)));
        assert_eq!(ids(&invented), ["TN0002", "TN0005"]);
        let real = apply(&devices, &view(|f| f.source = Some(DataSource::Dozent)));
        assert_eq!(ids(&real), ["TN0001", "TN0003", "TN0004"]);
    }

    #[test]
    fn pinned_devices_come_first_and_keep_the_inner_order() {
        let pinned: std::collections::BTreeSet<String> = ["TN0004".to_owned(), "TN0005".to_owned()].into();
        let result = apply_pinned(&fleet(), &ViewFilter::default(), &pinned);
        assert_eq!(ids(&result), ["TN0004", "TN0005", "TN0003", "TN0001", "TN0002"]);
        let none = apply_pinned(&fleet(), &ViewFilter::default(), &Default::default());
        assert_eq!(ids(&none), ids(&apply(&fleet(), &ViewFilter::default())));
    }

    #[test]
    fn counts_devices_by_source() {
        let mut devices = fleet();
        devices[0].data_source = DataSource::Simulation;
        assert_eq!(count_by_source(&devices), (4, 1));
        assert_eq!(count_by_source(&[]), (0, 0));
    }

    #[test]
    fn searches_id_and_name_case_insensitively() {
        assert_eq!(
            ids(&apply(&fleet(), &view(|f| f.search = "schloss".into()))),
            ["TN0002"]
        );
        assert_eq!(
            ids(&apply(&fleet(), &view(|f| f.search = " tn0004 ".into()))),
            ["TN0004"]
        );
        assert!(apply(&fleet(), &view(|f| f.search = "gibtsnicht".into())).is_empty());
        assert_eq!(apply(&fleet(), &view(|f| f.search = "  ".into())).len(), 5);
    }

    #[test]
    fn status_and_search_combine() {
        let result = apply(
            &fleet(),
            &view(|f| {
                f.status = Some(DeviceStatus::Normal);
                f.search = "neu".into();
            }),
        );
        assert_eq!(ids(&result), ["TN0005"]);
    }

    #[test]
    fn sorts_by_soc_with_unknown_last() {
        let asc = apply(&fleet(), &view(|f| f.sort = DeviceSort::SocPercent));
        assert_eq!(ids(&asc), ["TN0003", "TN0002", "TN0004", "TN0001", "TN0005"]);
        let desc = apply(
            &fleet(),
            &view(|f| {
                f.sort = DeviceSort::SocPercent;
                f.dir = SortDir::Desc;
            }),
        );
        assert_eq!(ids(&desc), ["TN0001", "TN0004", "TN0002", "TN0003", "TN0005"]);
    }

    #[test]
    fn sorts_by_last_seen_and_status_and_id() {
        let seen = apply(
            &fleet(),
            &view(|f| {
                f.sort = DeviceSort::LastSeenAt;
                f.dir = SortDir::Desc;
            }),
        );
        assert_eq!(ids(&seen), ["TN0001", "TN0003", "TN0002", "TN0004", "TN0005"]);
        let status = apply(&fleet(), &view(|f| f.sort = DeviceSort::Status));
        assert_eq!(ids(&status), ["TN0003", "TN0004", "TN0002", "TN0001", "TN0005"]);
        let id = apply(
            &fleet(),
            &view(|f| {
                f.sort = DeviceSort::Id;
                f.dir = SortDir::Desc;
            }),
        );
        assert_eq!(ids(&id), ["TN0005", "TN0004", "TN0003", "TN0002", "TN0001"]);
    }

    #[test]
    fn attention_first_keeps_the_chosen_order_inside_groups() {
        let result = apply(
            &fleet(),
            &ViewFilter {
                sort: DeviceSort::SocPercent,
                ..ViewFilter::default()
            },
        );
        assert_eq!(ids(&result), ["TN0003", "TN0004", "TN0002", "TN0001", "TN0005"]);
    }

    #[test]
    fn pages_hold_at_most_twenty_devices() {
        assert_eq!(PAGE_SIZE, 20);
        assert_eq!(page_bounds(0, 0, 20), (0, 0, 1), "leer: eine leere Seite");
        assert_eq!(page_bounds(15, 0, 20), (0, 15, 1));
        assert_eq!(page_bounds(20, 0, 20), (0, 20, 1));
        assert_eq!(page_bounds(21, 0, 20), (0, 20, 2));
        assert_eq!(page_bounds(21, 1, 20), (20, 21, 2));
        assert_eq!(page_bounds(87, 4, 20), (80, 87, 5));
    }

    #[test]
    fn a_page_beyond_the_end_falls_back_to_the_last_one() {
        assert_eq!(page_bounds(21, 9, 20), (20, 21, 2));
        assert_eq!(page_bounds(5, 3, 20), (0, 5, 1));
        assert_eq!(page_bounds(10, 0, 0), (0, 1, 10), "Größe 0 gilt als 1");
    }
}
