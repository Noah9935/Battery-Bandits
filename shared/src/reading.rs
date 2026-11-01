use serde::{Deserialize, Serialize};

/// Ein validierter Messwert eines TrafficNodes (F02).
///
/// Annahme bis zur Klärung mit dem Auftraggeber (README Frage 2): Der Ladestand
/// wird in Prozent übertragen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub device_id: String,
    /// Unix-Zeit in Millisekunden
    pub timestamp_ms: i64,
    /// Ladestand in Prozent, 0 bis 100
    pub charge_pct: f64,
}
