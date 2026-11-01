//! Gemeinsame Typen und reine Kernlogik für alle Dienste und die Web-App.
//!
//! Dieses Paket enthält keine Ein- und Ausgabe: Zeit wird als Parameter
//! übergeben, nichts wird gelesen oder geschrieben. Dadurch lässt sich die
//! Kernlogik (NF05) ohne Datenbank, Broker oder Browser testen und im Backend
//! wie im WebAssembly-Frontend verwenden.

pub mod api;
pub mod events;
pub mod reading;
pub mod status;
pub mod validation;

pub use reading::Reading;
pub use status::Status;
