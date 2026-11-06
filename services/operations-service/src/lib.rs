//! Meldungs- und Einsatz-Service.
//!
//! * [`rules`]: reine Entscheidung, was ein Statuswechsel für Meldungen bedeutet.
//! * [`repo`]: Datenbankoperationen für Meldungen und Wartungseinsätze.
//! * [`events`]: Reaktion auf Ereignisse aus `LISTEN domain_events`.
//! * [`http`]: REST-Schnittstelle (Axum), vom Dashboard-Service aufgerufen.

pub mod events;
pub mod http;
pub mod repo;
pub mod rules;
