//! Reine Regeln der Überwachung.

use chrono::{DateTime, Duration, Utc};
use shared::estimator::conservative_percent;
use shared::{DeviceStatus, Settings};

/// Status, den ein Gerät haben soll.
///
/// * Offline hat Vorrang: Wer länger als `offline_timeout_seconds` nichts gesendet hat, ist offline.
/// * Sonst entscheidet der Ladezustand: unter dem kritischen Schwellwert kritisch,
///   unter dem Warnschwellwert Warnung, sonst normal. Genau auf dem Schwellwert gilt noch die bessere Stufe.
/// * Ohne bekannten Ladezustand (frisch gewechselte Batterie) gilt normal.
pub fn target_status(
    soc_percent: Option<f32>,
    soc_sigma_percent: Option<f32>,
    last_seen_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    settings: &Settings,
) -> DeviceStatus {
    if let Some(seen) = last_seen_at
        && now - seen > Duration::seconds(i64::from(settings.offline_timeout_seconds))
    {
        return DeviceStatus::Offline;
    }
    match soc_percent.map(|soc| conservative_percent(soc, soc_sigma_percent)) {
        Some(soc) if soc < settings.threshold_critical_pct => DeviceStatus::Critical,
        Some(soc) if soc < settings.threshold_warning_pct => DeviceStatus::Warning,
        _ => DeviceStatus::Normal,
    }
}

/// Neuer Status, falls er sich ändert.
pub fn transition(
    current: DeviceStatus,
    soc_percent: Option<f32>,
    soc_sigma_percent: Option<f32>,
    last_seen_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    settings: &Settings,
) -> Option<DeviceStatus> {
    let target = target_status(soc_percent, soc_sigma_percent, last_seen_at, now, settings);
    (target != current).then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-06T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn seen(seconds_ago: i64) -> Option<DateTime<Utc>> {
        Some(now() - Duration::seconds(seconds_ago))
    }

    #[test]
    fn soc_maps_to_status_with_default_thresholds() {
        let s = Settings::default();
        let status = |soc| target_status(Some(soc), None, seen(10), now(), &s);
        assert_eq!(status(80.0), DeviceStatus::Normal);
        assert_eq!(status(39.9), DeviceStatus::Warning);
        assert_eq!(status(19.9), DeviceStatus::Critical);
        assert_eq!(status(0.0), DeviceStatus::Critical);
    }

    #[test]
    fn exact_thresholds_stay_in_the_better_class() {
        let s = Settings::default();
        assert_eq!(
            target_status(Some(40.0), None, seen(10), now(), &s),
            DeviceStatus::Normal
        );
        assert_eq!(
            target_status(Some(20.0), None, seen(10), now(), &s),
            DeviceStatus::Warning
        );
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let s = Settings {
            threshold_critical_pct: 30.0,
            threshold_warning_pct: 60.0,
            ..Settings::default()
        };
        assert_eq!(
            target_status(Some(25.0), None, seen(10), now(), &s),
            DeviceStatus::Critical
        );
        assert_eq!(
            target_status(Some(50.0), None, seen(10), now(), &s),
            DeviceStatus::Warning
        );
    }

    #[test]
    fn silence_beyond_timeout_means_offline_regardless_of_soc() {
        let s = Settings::default(); // 900 s
        assert_eq!(
            target_status(Some(90.0), None, seen(901), now(), &s),
            DeviceStatus::Offline
        );
        assert_eq!(
            target_status(Some(5.0), None, seen(5_000), now(), &s),
            DeviceStatus::Offline
        );
    }

    #[test]
    fn exactly_at_timeout_is_still_online() {
        assert_eq!(
            target_status(Some(90.0), None, seen(900), now(), &Settings::default()),
            DeviceStatus::Normal
        );
    }

    #[test]
    fn unknown_soc_is_normal_and_never_seen_is_not_offline() {
        let s = Settings::default();
        assert_eq!(target_status(None, None, seen(10), now(), &s), DeviceStatus::Normal);
        assert_eq!(target_status(Some(10.0), None, None, now(), &s), DeviceStatus::Critical);
    }

    #[test]
    fn an_uncertain_estimate_warns_earlier() {
        let s = Settings::default();
        // 45 Prozent geschätzt: sicher (Abweichung 2) ist das normal, unsicher (Abweichung 8) liegt der vorsichtige
        // Wert mit 37 unter der Warnschwelle.
        assert_eq!(
            target_status(Some(45.0), Some(2.0), seen(10), now(), &s),
            DeviceStatus::Normal
        );
        assert_eq!(
            target_status(Some(45.0), Some(8.0), seen(10), now(), &s),
            DeviceStatus::Warning
        );
        // 24 Prozent geschätzt und unsicher: vorsichtig 16, also kritisch.
        assert_eq!(
            target_status(Some(24.0), Some(8.0), seen(10), now(), &s),
            DeviceStatus::Critical
        );
        // Fehlende Angabe zur Unsicherheit ändert nichts am bisherigen Verhalten.
        assert_eq!(
            target_status(Some(24.0), None, seen(10), now(), &s),
            DeviceStatus::Warning
        );
    }

    #[test]
    fn transition_only_reports_changes() {
        let s = Settings::default();
        let t = |current, soc, ago| transition(current, Some(soc), None, seen(ago), now(), &s);
        assert_eq!(t(DeviceStatus::Normal, 50.0, 10), None);
        assert_eq!(t(DeviceStatus::Normal, 35.0, 10), Some(DeviceStatus::Warning));
        assert_eq!(t(DeviceStatus::Warning, 15.0, 10), Some(DeviceStatus::Critical));
        assert_eq!(t(DeviceStatus::Critical, 15.0, 10), None);
        assert_eq!(t(DeviceStatus::Critical, 15.0, 2_000), Some(DeviceStatus::Offline));
        assert_eq!(
            t(DeviceStatus::Offline, 15.0, 10),
            Some(DeviceStatus::Critical),
            "wieder online"
        );
        assert_eq!(t(DeviceStatus::Offline, 90.0, 10), Some(DeviceStatus::Normal));
        assert_eq!(
            t(DeviceStatus::Critical, 90.0, 10),
            Some(DeviceStatus::Normal),
            "Erholung nach Wechsel"
        );
    }
}
