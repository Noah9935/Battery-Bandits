//! Gemeinsame Typen und reine Kernlogik von Battery Bandits.
//!
//! Dieses Crate kennt weder Netzwerk noch Datenbank (außer den optionalen
//! sqlx-Ableitungen hinter dem Feature `sqlx`). Dadurch lässt es sich ohne
//! Hilfsmittel testen und auch nach WebAssembly übersetzen.

pub mod events;
pub mod models;
pub mod settings;
pub mod soc;
pub mod telemetry;

#[cfg(feature = "sqlx")]
pub mod db;

pub use events::DomainEvent;
pub use models::{
    Alert, AlertKind, AlertStatus, DashboardSummary, Device, DeviceStatus, MaintenanceStatus, MaintenanceTask, Reading,
};
pub use settings::{Settings, SettingsError};
pub use telemetry::{TelemetryPayload, ValidationError, node_id_from_topic};
