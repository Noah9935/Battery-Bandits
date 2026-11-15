//! Meldungs- und Einsatz-Service.
//!
//! * [`rules`]: reine Entscheidung, was ein Statuswechsel für Meldungen bedeutet.
//! * [`repo`]: Datenbankoperationen für Meldungen und Wartungseinsätze.
//! * [`events`]: Reaktion auf Ereignisse aus `LISTEN domain_events`.
//! * [`http`]: REST-Schnittstelle (Axum), vom Dashboard-Service aufgerufen.
//! * [`notify`]: Versand neuer Meldungen aufs Handy (Telegram, WhatsApp, SMS, E-Mail).
//! * [`mail`]: die gestaltete E-Mail zu einer Meldung (Text und HTML, mit Logo).

pub mod events;
pub mod http;
pub mod mail;
pub mod notify;
pub mod repo;
pub mod rules;
