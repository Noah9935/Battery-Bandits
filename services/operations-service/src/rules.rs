//! Was bedeutet ein Statuswechsel für Meldungen? (rein, ohne Datenbank)

use shared::{AlertKind, DeviceStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertAction {
    /// Meldung eröffnen (falls noch keine offene existiert).
    Open(AlertKind),
    /// Offene Meldungen des Geräts schließen: Das Problem ist behoben.
    Resolve,
}

/// Kritisch oder offline eröffnet eine Meldung, normal oder Warnung schließt offene Meldungen.
/// Eine Warnung ist kein Handlungsbedarf mehr: Die Batterie hat sich erholt.
pub fn action_for(new_status: DeviceStatus) -> AlertAction {
    match new_status {
        DeviceStatus::Critical => AlertAction::Open(AlertKind::Critical),
        DeviceStatus::Offline => AlertAction::Open(AlertKind::Offline),
        DeviceStatus::Normal | DeviceStatus::Warning => AlertAction::Resolve,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_and_offline_open_alerts_of_matching_kind() {
        assert_eq!(
            action_for(DeviceStatus::Critical),
            AlertAction::Open(AlertKind::Critical)
        );
        assert_eq!(action_for(DeviceStatus::Offline), AlertAction::Open(AlertKind::Offline));
    }

    #[test]
    fn recovery_resolves() {
        assert_eq!(action_for(DeviceStatus::Normal), AlertAction::Resolve);
        assert_eq!(action_for(DeviceStatus::Warning), AlertAction::Resolve);
    }

    #[test]
    fn every_status_has_an_action_consistent_with_needs_attention() {
        for status in DeviceStatus::ALL {
            let opens = matches!(action_for(*status), AlertAction::Open(_));
            assert_eq!(opens, status.needs_attention(), "{status:?}");
        }
    }
}
