//! Einstellbare Systemwerte (NF07: ohne Codeänderung anpassbar).

use serde::{Deserialize, Serialize};

/// Zeile der Tabelle `system_settings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Settings {
    pub threshold_critical_pct: f32,
    pub threshold_warning_pct: f32,
    pub offline_timeout_seconds: i32,
    pub min_voltage_mv: i32,
    pub max_voltage_mv: i32,
    pub battery_capacity_ah: f32,
}

impl Default for Settings {
    /// Vorgaben aus Phasenplan und Vorlesung. Offline nach 15 Minuten und die
    /// Ruhespannungsgrenzen sind Annahmen, die mit dem Auftraggeber zu bestätigen sind.
    fn default() -> Self {
        Self {
            threshold_critical_pct: 20.0,
            threshold_warning_pct: 40.0,
            offline_timeout_seconds: 900,
            min_voltage_mv: 10_500,
            max_voltage_mv: 12_700,
            battery_capacity_ah: 80.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SettingsError {
    #[error("Schwellwerte müssen zwischen 0 und 100 Prozent liegen")]
    ThresholdOutOfRange,
    #[error("Der kritische Schwellwert muss kleiner als der Warnschwellwert sein")]
    ThresholdOrder,
    #[error("Die Offline-Zeit muss mindestens 60 Sekunden betragen")]
    OfflineTimeoutTooShort,
    #[error("Die Mindestspannung muss kleiner als die Höchstspannung sein")]
    VoltageOrder,
    #[error("Die Nennkapazität muss größer als 0 sein")]
    CapacityNotPositive,
}

impl Settings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        let pct = 0.0..=100.0;
        if !pct.contains(&self.threshold_critical_pct) || !pct.contains(&self.threshold_warning_pct) {
            return Err(SettingsError::ThresholdOutOfRange);
        }
        if self.threshold_critical_pct >= self.threshold_warning_pct {
            return Err(SettingsError::ThresholdOrder);
        }
        if self.offline_timeout_seconds < 60 {
            return Err(SettingsError::OfflineTimeoutTooShort);
        }
        if self.min_voltage_mv >= self.max_voltage_mv {
            return Err(SettingsError::VoltageOrder);
        }
        if self.battery_capacity_ah.is_nan() || self.battery_capacity_ah <= 0.0 {
            return Err(SettingsError::CapacityNotPositive);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_match_plan() {
        let s = Settings::default();
        assert_eq!(s.validate(), Ok(()));
        assert_eq!((s.threshold_critical_pct, s.threshold_warning_pct), (20.0, 40.0));
        assert_eq!(s.battery_capacity_ah, 80.0);
    }

    #[test]
    fn rejects_bad_thresholds() {
        let s = Settings {
            threshold_critical_pct: 50.0,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::ThresholdOrder));
        let s = Settings {
            threshold_warning_pct: 120.0,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::ThresholdOutOfRange));
        let s = Settings {
            threshold_critical_pct: -1.0,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::ThresholdOutOfRange));
    }

    #[test]
    fn rejects_bad_timeout_voltage_and_capacity() {
        let s = Settings {
            offline_timeout_seconds: 30,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::OfflineTimeoutTooShort));
        let s = Settings {
            min_voltage_mv: 13_000,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::VoltageOrder));
        let s = Settings {
            battery_capacity_ah: 0.0,
            ..Settings::default()
        };
        assert_eq!(s.validate(), Err(SettingsError::CapacityNotPositive));
    }
}
