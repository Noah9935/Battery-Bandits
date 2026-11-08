//! Gemeinsame Typen und reine Kernlogik von Battery Bandits.
//!
//! Dieses Crate kennt weder Netzwerk noch Datenbank (außer den optionalen
//! sqlx-Ableitungen hinter dem Feature `sqlx`). Dadurch lässt es sich ohne
//! Hilfsmittel testen und auch nach WebAssembly übersetzen.

pub mod api;
pub mod events;
pub mod forecast;
pub mod models;
pub mod settings;
pub mod soc;
pub mod telemetry;

#[cfg(feature = "sqlx")]
pub mod db;

pub use events::DomainEvent;
pub use forecast::Forecast;
pub use models::{
    ActivityEntry, Alert, AlertKind, AlertStatus, AssignTask, DashboardSummary, DataSource, Device, DeviceStatus,
    InboxItem, MaintenanceStatus, MaintenanceTask, MarkRead, NewMaintenanceTask, NewTaskComment, Reading, TaskComment,
    TaskKind, TaskPriority,
};
pub use settings::{AnomalySettings, Settings, SettingsError};
pub use telemetry::{TelemetryPayload, ValidationError, node_id_from_topic};
