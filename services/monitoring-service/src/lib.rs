//! Überwachungs-Service (Event Storming: Schwellwert prüfen, Gerät als kritisch oder offline markieren).
//!
//! * [`rules`]: reine Regeln (Status aus Ladezustand und Zeit).
//! * [`service`]: Datenbankoperationen (bewerten, Prognose, Batteriewechsel).
//! * [`runner`]: Schleife aus `LISTEN domain_events` und zwei Zeitgebern.

pub mod rules;
pub mod runner;
pub mod service;
