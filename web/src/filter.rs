//! Filtern und Sortieren der Geräteübersicht (F13, NF04).
//!
//! Bei rund 500 Geräten ist die ganze Liste klein genug, um sie im Browser zu
//! bearbeiten. Das hält die Oberfläche ohne Wartezeit und die API einfach.

use shared::Status;
use shared::api::DeviceSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    #[default]
    All,
    Critical,
    Ok,
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 3] = [StatusFilter::All, StatusFilter::Critical, StatusFilter::Ok];

    pub fn key(self) -> &'static str {
        match self {
            StatusFilter::All => "all",
            StatusFilter::Critical => "critical",
            StatusFilter::Ok => "ok",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "critical" => StatusFilter::Critical,
            "ok" => StatusFilter::Ok,
            _ => StatusFilter::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "Alle",
            StatusFilter::Critical => "Kritisch",
            StatusFilter::Ok => "In Ordnung",
        }
    }

    fn matches(self, status: Status) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Critical => status == Status::Critical,
            StatusFilter::Ok => status == Status::Ok,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    /// Kritische zuerst, darunter nach Ladestand: So sind kritische Geräte ohne Suche erkennbar (NF04).
    #[default]
    CriticalFirst,
    ChargeAsc,
    ChargeDesc,
    LatestUpdate,
    OldestUpdate,
    Id,
}

impl SortKey {
    pub const ALL: [SortKey; 6] = [
        SortKey::CriticalFirst,
        SortKey::ChargeAsc,
        SortKey::ChargeDesc,
        SortKey::LatestUpdate,
        SortKey::OldestUpdate,
        SortKey::Id,
    ];

    pub fn key(self) -> &'static str {
        match self {
            SortKey::CriticalFirst => "critical_first",
            SortKey::ChargeAsc => "charge_asc",
            SortKey::ChargeDesc => "charge_desc",
            SortKey::LatestUpdate => "latest",
            SortKey::OldestUpdate => "oldest",
            SortKey::Id => "id",
        }
    }

    pub fn from_key(key: &str) -> Self {
        SortKey::ALL.into_iter().find(|s| s.key() == key).unwrap_or_default()
    }

    pub fn label(self) -> &'static str {
        match self {
            SortKey::CriticalFirst => "Kritische zuerst",
            SortKey::ChargeAsc => "Ladestand aufsteigend",
            SortKey::ChargeDesc => "Ladestand absteigend",
            SortKey::LatestUpdate => "Zuletzt aktualisiert",
            SortKey::OldestUpdate => "Am längsten ohne Update",
            SortKey::Id => "Geräte-ID",
        }
    }
}

fn charge(d: &DeviceSummary) -> f64 {
    d.charge_pct.unwrap_or(f64::MAX)
}

fn updated(d: &DeviceSummary) -> i64 {
    d.last_reading_ms.unwrap_or(i64::MIN)
}

/// Wendet Suche, Statusfilter und Sortierung an. Die Suche beachtet keine Groß- und Kleinschreibung.
pub fn apply(devices: &[DeviceSummary], query: &str, filter: StatusFilter, sort: SortKey) -> Vec<DeviceSummary> {
    let needle = query.trim().to_lowercase();
    let mut out: Vec<DeviceSummary> = devices
        .iter()
        .filter(|d| filter.matches(d.status))
        .filter(|d| needle.is_empty() || d.id.to_lowercase().contains(&needle))
        .cloned()
        .collect();

    // Gleichstände fallen immer auf die Geräte-ID zurück, damit die Reihenfolge stabil bleibt.
    out.sort_by(|a, b| {
        let primary = match sort {
            SortKey::CriticalFirst => ((a.status != Status::Critical) as u8)
                .cmp(&((b.status != Status::Critical) as u8))
                .then(charge(a).total_cmp(&charge(b))),
            SortKey::ChargeAsc => charge(a).total_cmp(&charge(b)),
            SortKey::ChargeDesc => charge(b).total_cmp(&charge(a)),
            SortKey::LatestUpdate => updated(b).cmp(&updated(a)),
            SortKey::OldestUpdate => updated(a).cmp(&updated(b)),
            SortKey::Id => std::cmp::Ordering::Equal,
        };
        primary.then_with(|| a.id.cmp(&b.id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, charge: f64, ts: i64, status: Status) -> DeviceSummary {
        DeviceSummary {
            id: id.into(),
            charge_pct: Some(charge),
            last_reading_ms: Some(ts),
            status,
            open_incident: false,
        }
    }

    fn fleet() -> Vec<DeviceSummary> {
        vec![
            device("TN-0003", 80.0, 300, Status::Ok),
            device("TN-0001", 10.0, 100, Status::Critical),
            device("TN-0002", 50.0, 200, Status::Ok),
            device("TN-0004", 15.0, 400, Status::Critical),
        ]
    }

    fn ids(list: &[DeviceSummary]) -> Vec<&str> {
        list.iter().map(|d| d.id.as_str()).collect()
    }

    #[test]
    fn default_sort_puts_critical_first_then_lowest_charge() {
        let out = apply(&fleet(), "", StatusFilter::All, SortKey::CriticalFirst);
        assert_eq!(ids(&out), ["TN-0001", "TN-0004", "TN-0002", "TN-0003"]);
    }

    #[test]
    fn filters_by_status() {
        let critical = apply(&fleet(), "", StatusFilter::Critical, SortKey::Id);
        assert_eq!(ids(&critical), ["TN-0001", "TN-0004"]);
        let ok = apply(&fleet(), "", StatusFilter::Ok, SortKey::Id);
        assert_eq!(ids(&ok), ["TN-0002", "TN-0003"]);
    }

    #[test]
    fn searches_case_insensitive_and_trimmed() {
        let out = apply(&fleet(), "  tn-0002 ", StatusFilter::All, SortKey::Id);
        assert_eq!(ids(&out), ["TN-0002"]);
        assert_eq!(apply(&fleet(), "000", StatusFilter::All, SortKey::Id).len(), 4);
        assert!(apply(&fleet(), "xyz", StatusFilter::All, SortKey::Id).is_empty());
    }

    #[test]
    fn search_and_filter_combine() {
        let out = apply(&fleet(), "0004", StatusFilter::Ok, SortKey::Id);
        assert!(out.is_empty());
    }

    #[test]
    fn sorts_by_charge() {
        let asc = apply(&fleet(), "", StatusFilter::All, SortKey::ChargeAsc);
        assert_eq!(ids(&asc), ["TN-0001", "TN-0004", "TN-0002", "TN-0003"]);
        let desc = apply(&fleet(), "", StatusFilter::All, SortKey::ChargeDesc);
        assert_eq!(ids(&desc), ["TN-0003", "TN-0002", "TN-0004", "TN-0001"]);
    }

    #[test]
    fn sorts_by_update_time() {
        let latest = apply(&fleet(), "", StatusFilter::All, SortKey::LatestUpdate);
        assert_eq!(ids(&latest), ["TN-0004", "TN-0003", "TN-0002", "TN-0001"]);
        let oldest = apply(&fleet(), "", StatusFilter::All, SortKey::OldestUpdate);
        assert_eq!(ids(&oldest), ["TN-0001", "TN-0002", "TN-0003", "TN-0004"]);
    }

    #[test]
    fn ties_fall_back_to_id() {
        let mut list = vec![
            device("TN-0002", 50.0, 1, Status::Ok),
            device("TN-0001", 50.0, 1, Status::Ok),
        ];
        list = apply(&list, "", StatusFilter::All, SortKey::ChargeAsc);
        assert_eq!(ids(&list), ["TN-0001", "TN-0002"]);
    }

    #[test]
    fn devices_without_values_sort_last_by_charge() {
        let mut list = fleet();
        list.push(DeviceSummary {
            id: "TN-0005".into(),
            charge_pct: None,
            last_reading_ms: None,
            status: Status::Ok,
            open_incident: false,
        });
        let out = apply(&list, "", StatusFilter::All, SortKey::ChargeAsc);
        assert_eq!(out.last().map(|d| d.id.as_str()), Some("TN-0005"));
    }

    #[test]
    fn keys_roundtrip() {
        for f in StatusFilter::ALL {
            assert_eq!(StatusFilter::from_key(f.key()), f);
        }
        for s in SortKey::ALL {
            assert_eq!(SortKey::from_key(s.key()), s);
        }
        assert_eq!(StatusFilter::from_key("quatsch"), StatusFilter::All);
        assert_eq!(SortKey::from_key("quatsch"), SortKey::CriticalFirst);
    }
}
