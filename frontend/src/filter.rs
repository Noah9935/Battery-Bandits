//! Filtern und Sortieren der Geräteliste im Browser.
//!
//! Gleiche Regeln wie im Dashboard-Service (`/api/devices`), damit beide dasselbe Ergebnis liefern.

use std::cmp::Ordering;

use shared::api::{DeviceSort, SortDir};
use shared::{Device, DeviceStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct ViewFilter {
    pub status: Option<DeviceStatus>,
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
            search: String::new(),
            sort: DeviceSort::Id,
            dir: SortDir::Asc,
            attention_first: true,
        }
    }
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
        .filter(|d| matches_search(d, &filter.search))
        .cloned()
        .collect();
    result.sort_by(|a, b| {
        let attention = if filter.attention_first {
            (!a.status.needs_attention()).cmp(&!b.status.needs_attention())
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
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: at(0),
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
}
