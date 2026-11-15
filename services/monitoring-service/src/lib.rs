//! Überwachungs-Service (Event Storming: Schwellwert prüfen, Gerät als kritisch oder offline markieren).
//!
//! * [`anomaly`]: Stillstand und Spannungseinbruch erkennen.
//! * [`rules`]: reine Regeln (Status aus Ladezustand und Zeit).
//! * [`service`]: Datenbankoperationen (bewerten, Prognose, Batteriewechsel).
//! * [`runner`]: Schleife aus `LISTEN domain_events` und zwei Zeitgebern.
//! * [`retention`]: Deckel für die Datenbank, löscht bei 95 Prozent des Platzes die ältesten Messwerte.

pub mod anomaly;
pub mod retention;
pub mod rules;
pub mod runner;
pub mod service;
