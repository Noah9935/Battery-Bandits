//! Erfassungs-Service (Event Storming: Nachricht empfangen, prüfen, Messwert speichern).
//!
//! Ablauf: MQTT-Nachricht → [`classify::classify`] → [`observer::Dispatcher`] → Beobachter.
//! Der [`store::PostgresObserver`] speichert gültige Messwerte und protokolliert
//! ungültige. Die Kernlogik (`classify`, `soc_state`, `config`) ist rein und ohne
//! Netzwerk testbar.

pub mod classify;
pub mod config;
pub mod import;
pub mod mqtt;
pub mod observer;
pub mod soc_state;
pub mod store;
