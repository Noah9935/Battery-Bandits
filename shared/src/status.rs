use serde::{Deserialize, Serialize};

/// Zustand eines Geräts (F05).
///
/// Offline (F07) gehört zur Future Work und kommt später als weitere Variante dazu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Status {
    Ok,
    Critical,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Critical => "critical",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "ok" => Some(Status::Ok),
            "critical" => Some(Status::Critical),
            _ => None,
        }
    }
}

/// Bewertet einen Ladestand gegen den Schwellwert. Kritisch ist, wer unter dem
/// Schwellwert liegt; genau auf dem Schwellwert gilt noch als in Ordnung.
pub fn evaluate(charge_pct: f64, critical_threshold_pct: f64) -> Status {
    if charge_pct < critical_threshold_pct {
        Status::Critical
    } else {
        Status::Ok
    }
}

/// Was sich beim Wechsel von einem Zustand in den nächsten ergibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// Keine Änderung, die gemeldet werden müsste
    Unchanged,
    /// Gerät ist neu kritisch (Ereignis "Gerät kritisch")
    BecameCritical,
    /// Gerät war kritisch und ist wieder normal (Ereignis "Gerät wieder normal")
    Recovered,
}

/// Leitet aus dem bisherigen und dem neuen Zustand das Ereignis ab. Ein Gerät,
/// das gleich mit einem normalen Wert startet, löst nichts aus.
pub fn transition(previous: Option<Status>, current: Status) -> Transition {
    match (previous, current) {
        (Some(Status::Critical), Status::Ok) => Transition::Recovered,
        (Some(Status::Critical), Status::Critical) => Transition::Unchanged,
        (_, Status::Critical) => Transition::BecameCritical,
        _ => Transition::Unchanged,
    }
}

/// Zulässiger Bereich für den konfigurierbaren Schwellwert.
pub fn is_valid_threshold(value: f64) -> bool {
    value.is_finite() && (1.0..=99.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_threshold_is_critical() {
        assert_eq!(evaluate(19.9, 20.0), Status::Critical);
        assert_eq!(evaluate(0.0, 20.0), Status::Critical);
    }

    #[test]
    fn at_or_above_threshold_is_ok() {
        assert_eq!(evaluate(20.0, 20.0), Status::Ok);
        assert_eq!(evaluate(100.0, 20.0), Status::Ok);
    }

    #[test]
    fn threshold_range() {
        assert!(is_valid_threshold(20.0));
        assert!(is_valid_threshold(1.0));
        assert!(is_valid_threshold(99.0));
        assert!(!is_valid_threshold(0.5));
        assert!(!is_valid_threshold(100.0));
        assert!(!is_valid_threshold(f64::NAN));
        assert!(!is_valid_threshold(f64::INFINITY));
    }

    #[test]
    fn transitions() {
        use Transition::*;
        assert_eq!(transition(None, Status::Critical), BecameCritical);
        assert_eq!(transition(Some(Status::Ok), Status::Critical), BecameCritical);
        assert_eq!(transition(Some(Status::Critical), Status::Critical), Unchanged);
        assert_eq!(transition(Some(Status::Critical), Status::Ok), Recovered);
        assert_eq!(transition(Some(Status::Ok), Status::Ok), Unchanged);
        assert_eq!(transition(None, Status::Ok), Unchanged);
    }

    #[test]
    fn db_roundtrip() {
        for s in [Status::Ok, Status::Critical] {
            assert_eq!(Status::from_db(s.as_str()), Some(s));
        }
        assert_eq!(Status::from_db("offline"), None);
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Status::Critical).unwrap(), "\"critical\"");
    }
}
