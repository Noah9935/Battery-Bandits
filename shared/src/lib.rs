//! Gemeinsame Typen für ingest, api und web.

/// Ein validierter Messwert eines TrafficNodes (F02).
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub device_id: String,
    /// Unix-Zeit in Millisekunden
    pub timestamp_ms: i64,
    /// Ladestand; Einheit (% oder Volt) noch offen
    pub charge: f64,
}
