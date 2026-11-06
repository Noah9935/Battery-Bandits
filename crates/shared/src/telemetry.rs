//! Nachrichtenformat der TrafficNodes und seine Prüfung (F06).
//!
//! Die Nutzlast entspricht `sample_msg.json` der Vorlesung. Position, Höhe,
//! Satelliten und Zeit sind oft `null` (kein GPS-Fix). Die Messgeräte liefern
//! Strom und Spannung teils mit Nachkommastellen (`12296.25`), deshalb werden
//! diese Felder tolerant gelesen und auf ganze Milliampere beziehungsweise
//! Millivolt gerundet.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// Plausible Grenzen. Alles außerhalb gilt als Übertragungs- oder Messfehler.
pub const VOLTAGE_MV_RANGE: (i32, i32) = (1, 60_000);
pub const CURRENT_MA_ABS_MAX: i32 = 500_000;
pub const TEMPERATURE_C_RANGE: (f32, f32) = (-60.0, 150.0);
pub const ALTITUDE_M_RANGE: (f64, f64) = (-500.0, 10_000.0);

/// Eine Messung eines TrafficNodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryPayload {
    #[serde(rename = "current_mA", deserialize_with = "de_rounded_i32")]
    pub current_ma: i32,
    #[serde(rename = "voltage_mV", deserialize_with = "de_rounded_i32")]
    pub voltage_mv: i32,
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    #[serde(default)]
    pub altitude_m: Option<f64>,
    #[serde(default)]
    pub satellites: Option<u8>,
    #[serde(default)]
    pub time: Option<DateTime<Utc>>,
    /// Ältere Firmware sendet keine Temperatur (echte Historie von Gerät 101), deshalb optional.
    #[serde(rename = "temperature_C", default)]
    pub temperature_c: Option<f32>,
}

/// Liest eine Zahl mit oder ohne Nachkommastellen als gerundeten `i32`.
fn de_rounded_i32<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i32, D::Error> {
    let value = f64::deserialize(deserializer)?;
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(serde::de::Error::custom("Zahl außerhalb des Wertebereichs"));
    }
    Ok(value.round() as i32)
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValidationError {
    #[error("Spannung {0} mV außerhalb des plausiblen Bereichs")]
    VoltageOutOfRange(i32),
    #[error("Strom {0} mA außerhalb des plausiblen Bereichs")]
    CurrentOutOfRange(i32),
    #[error("Temperatur {0} °C außerhalb des plausiblen Bereichs")]
    TemperatureOutOfRange(f32),
    #[error("Breitengrad {0} ungültig")]
    LatitudeOutOfRange(f64),
    #[error("Längengrad {0} ungültig")]
    LongitudeOutOfRange(f64),
    #[error("Höhe {0} m ungültig")]
    AltitudeOutOfRange(f64),
    #[error("Position unvollständig: Breiten- und Längengrad nur gemeinsam")]
    IncompletePosition,
}

impl TelemetryPayload {
    /// Prüft die Wertebereiche. Fehlende GPS-Werte sind erlaubt, halbe Positionen nicht.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(VOLTAGE_MV_RANGE.0..=VOLTAGE_MV_RANGE.1).contains(&self.voltage_mv) {
            return Err(ValidationError::VoltageOutOfRange(self.voltage_mv));
        }
        if self.current_ma.unsigned_abs() > CURRENT_MA_ABS_MAX.unsigned_abs() {
            return Err(ValidationError::CurrentOutOfRange(self.current_ma));
        }
        if let Some(t) = self.temperature_c
            && (!t.is_finite() || !(TEMPERATURE_C_RANGE.0..=TEMPERATURE_C_RANGE.1).contains(&t))
        {
            return Err(ValidationError::TemperatureOutOfRange(t));
        }
        match (self.lat, self.lon) {
            (None, None) => {}
            (Some(lat), Some(lon)) => {
                if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
                    return Err(ValidationError::LatitudeOutOfRange(lat));
                }
                if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
                    return Err(ValidationError::LongitudeOutOfRange(lon));
                }
            }
            _ => return Err(ValidationError::IncompletePosition),
        }
        if let Some(alt) = self.altitude_m
            && (!alt.is_finite() || !(ALTITUDE_M_RANGE.0..=ALTITUDE_M_RANGE.1).contains(&alt))
        {
            return Err(ValidationError::AltitudeOutOfRange(alt));
        }
        Ok(())
    }

    /// Hat die Nachricht einen GPS-Fix?
    pub fn has_position(&self) -> bool {
        self.lat.is_some() && self.lon.is_some()
    }
}

/// Gerätekennung aus dem Topic: das zweite von genau drei Segmenten,
/// zum Beispiel `traffic-nodes/TN0001/measurements` oder `trafficnode/TN0001/telemetry`.
pub fn node_id_from_topic(topic: &str) -> Option<&str> {
    let mut parts = topic.split('/');
    let (_, id, _, rest) = (parts.next()?, parts.next()?, parts.next()?, parts.next());
    let valid = rest.is_none() && !id.is_empty() && !id.contains(['+', '#']) && id.len() <= 64;
    valid.then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> TelemetryPayload {
        TelemetryPayload {
            current_ma: -311,
            voltage_mv: 12_296,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            time: None,
            temperature_c: Some(16.44),
        }
    }

    #[test]
    fn parses_sample_message_with_null_fields() {
        let json = r#"{"current_mA":-311.25,"voltage_mV":12296.25,"lat":null,"lon":null,
            "altitude_m":null,"satellites":null,"time":null,"temperature_C":16.44}"#;
        let p: TelemetryPayload = serde_json::from_str(json).unwrap();
        assert_eq!(p.current_ma, -311);
        assert_eq!(p.voltage_mv, 12_296);
        assert_eq!(
            (p.lat, p.lon, p.altitude_m, p.satellites, p.time),
            (None, None, None, None, None)
        );
        assert!(!p.has_position());
        assert!((p.temperature_c.unwrap() - 16.44).abs() < 1e-4);
    }

    #[test]
    fn parses_message_with_gps_and_time() {
        let json = r#"{"current_mA":120,"voltage_mV":12500,"lat":48.8,"lon":9.0,"altitude_m":321.5,
            "satellites":7,"time":"2026-11-05T11:30:00Z","temperature_C":-3.5}"#;
        let p: TelemetryPayload = serde_json::from_str(json).unwrap();
        assert_eq!(p.satellites, Some(7));
        assert!(p.has_position());
        assert_eq!(p.time.unwrap().to_rfc3339(), "2026-11-05T11:30:00+00:00");
        assert!(p.validate().is_ok());
    }

    #[test]
    fn missing_optional_fields_are_none() {
        let json = r#"{"current_mA":1,"voltage_mV":12000,"temperature_C":20}"#;
        let p: TelemetryPayload = serde_json::from_str(json).unwrap();
        assert_eq!(p.lat, None);
        assert_eq!(p.time, None);
    }

    #[test]
    fn rejects_null_or_missing_required_fields() {
        let null_voltage = r#"{"current_mA":1,"voltage_mV":null,"temperature_C":20}"#;
        assert!(serde_json::from_str::<TelemetryPayload>(null_voltage).is_err());
        let missing_voltage = r#"{"current_mA":1,"temperature_C":20}"#;
        assert!(serde_json::from_str::<TelemetryPayload>(missing_voltage).is_err());
        assert!(serde_json::from_str::<TelemetryPayload>("not json").is_err());
    }

    #[test]
    fn temperature_is_optional_for_older_firmware() {
        // So sehen die echten Nachrichten von Gerät 101 aus der Historie aus: kein Feld temperature_C.
        let json = r#"{"altitude_m":null,"current_mA":-290.0,"lat":null,"lon":null,"satellites":null,"time":null,"voltage_mV":12710.0}"#;
        let p: TelemetryPayload = serde_json::from_str(json).unwrap();
        assert_eq!(p.temperature_c, None);
        assert_eq!(p.validate(), Ok(()));
        let null_temp = r#"{"current_mA":1,"voltage_mV":12000,"temperature_C":null}"#;
        assert_eq!(
            serde_json::from_str::<TelemetryPayload>(null_temp)
                .unwrap()
                .temperature_c,
            None
        );
    }

    #[test]
    fn serialization_uses_wire_names() {
        let value = serde_json::to_value(payload()).unwrap();
        assert_eq!(value["current_mA"], -311);
        assert_eq!(value["voltage_mV"], 12_296);
        assert!(value["temperature_C"].is_number());
        assert!(value["lat"].is_null());
    }

    #[test]
    fn valid_payload_passes() {
        assert_eq!(payload().validate(), Ok(()));
    }

    #[test]
    fn rejects_voltage_out_of_range() {
        for v in [0, -5, 60_001] {
            let p = TelemetryPayload {
                voltage_mv: v,
                ..payload()
            };
            assert_eq!(p.validate(), Err(ValidationError::VoltageOutOfRange(v)));
        }
    }

    #[test]
    fn rejects_current_out_of_range() {
        let p = TelemetryPayload {
            current_ma: 600_000,
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::CurrentOutOfRange(600_000)));
        let p = TelemetryPayload {
            current_ma: i32::MIN,
            ..payload()
        };
        assert!(p.validate().is_err());
    }

    #[test]
    fn rejects_bad_temperature() {
        for t in [-61.0, 151.0, f32::NAN] {
            let p = TelemetryPayload {
                temperature_c: Some(t),
                ..payload()
            };
            assert!(matches!(p.validate(), Err(ValidationError::TemperatureOutOfRange(_))));
        }
    }

    #[test]
    fn rejects_half_position() {
        let p = TelemetryPayload {
            lat: Some(48.0),
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::IncompletePosition));
        let p = TelemetryPayload {
            lon: Some(9.0),
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::IncompletePosition));
    }

    #[test]
    fn rejects_position_out_of_range() {
        let p = TelemetryPayload {
            lat: Some(91.0),
            lon: Some(9.0),
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::LatitudeOutOfRange(91.0)));
        let p = TelemetryPayload {
            lat: Some(48.0),
            lon: Some(181.0),
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::LongitudeOutOfRange(181.0)));
    }

    #[test]
    fn rejects_altitude_out_of_range() {
        let p = TelemetryPayload {
            altitude_m: Some(99_999.0),
            ..payload()
        };
        assert_eq!(p.validate(), Err(ValidationError::AltitudeOutOfRange(99_999.0)));
    }

    #[test]
    fn extracts_node_id_from_both_topic_styles() {
        assert_eq!(node_id_from_topic("traffic-nodes/TN0001/measurements"), Some("TN0001"));
        assert_eq!(node_id_from_topic("trafficnode/node-7/telemetry"), Some("node-7"));
    }

    #[test]
    fn rejects_unusable_topics() {
        for topic in ["", "a", "a/b", "a//c", "a/b/c/d", "a/+/c", "a/#/c"] {
            assert_eq!(node_id_from_topic(topic), None, "{topic}");
        }
    }
}
