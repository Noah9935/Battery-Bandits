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

/// Schwellen für die Erkennung von Auffälligkeiten (Tabelle `anomaly_settings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct AnomalySettings {
    pub enabled: bool,
    /// So lange dürfen Spannung und Strom unverändert bleiben, bevor der Verdacht "Sensor hängt" entsteht.
    pub stagnation_hours: f32,
    /// So stark darf die Spannung innerhalb des Fensters fallen.
    pub drop_mv: i32,
    pub drop_window_minutes: i32,
    /// Ab diesem Ladestand gilt ein ausgefallenes Gerät als technischer Defekt statt als leere Batterie.
    pub fault_min_pct: f32,
}

impl Default for AnomalySettings {
    /// Annahmen, die mit dem Auftraggeber zu bestätigen sind.
    fn default() -> Self {
        Self {
            enabled: true,
            stagnation_hours: 6.0,
            drop_mv: 600,
            drop_window_minutes: 60,
            fault_min_pct: 50.0,
        }
    }
}

impl AnomalySettings {
    pub fn validate(&self) -> Result<(), SettingsError> {
        if !(1.0..=168.0).contains(&self.stagnation_hours) {
            return Err(SettingsError::StagnationRange);
        }
        if !(50..=5000).contains(&self.drop_mv) {
            return Err(SettingsError::DropRange);
        }
        if !(5..=1440).contains(&self.drop_window_minutes) {
            return Err(SettingsError::DropWindowRange);
        }
        if !(0.0..=100.0).contains(&self.fault_min_pct) {
            return Err(SettingsError::ThresholdOutOfRange);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SettingsError {
    #[error("Der Stillstand muss zwischen 1 und 168 Stunden liegen")]
    StagnationRange,
    #[error("Der Spannungseinbruch muss zwischen 50 und 5000 mV liegen")]
    DropRange,
    #[error("Das Zeitfenster für den Spannungseinbruch muss zwischen 5 und 1440 Minuten liegen")]
    DropWindowRange,
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
