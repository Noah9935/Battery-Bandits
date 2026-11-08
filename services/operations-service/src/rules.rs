//! Was bedeutet ein Statuswechsel für Meldungen? (rein, ohne Datenbank)

use shared::{AlertKind, AnomalySettings, DeviceStatus};

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

/// Ein ausgefallenes Gerät, das vorher gut geladen war, ist eher ein technischer Defekt oder ein Funkloch
/// als eine leere Batterie. Das ändert, wer sich darum kümmern sollte.
pub fn offline_kind(last_soc: Option<f32>, settings: &AnomalySettings) -> AlertKind {
    match last_soc {
        Some(soc) if settings.enabled && soc >= settings.fault_min_pct => AlertKind::Fault,
        _ => AlertKind::Offline,
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
    fn a_well_charged_device_that_went_silent_is_a_fault() {
        let s = AnomalySettings::default();
        assert_eq!(offline_kind(Some(50.0), &s), AlertKind::Fault);
        assert_eq!(offline_kind(Some(95.0), &s), AlertKind::Fault);
        assert_eq!(offline_kind(Some(49.9), &s), AlertKind::Offline);
        assert_eq!(
            offline_kind(None, &s),
            AlertKind::Offline,
            "ohne bekannten Ladestand bleibt es bei offline"
        );
        let off = AnomalySettings {
            enabled: false,
            ..AnomalySettings::default()
        };
        assert_eq!(offline_kind(Some(95.0), &off), AlertKind::Offline);
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
