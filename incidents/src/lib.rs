//! Meldungs-Service.
//!
//! Reagiert auf die Ereignisse der Überwachung (Event Storming 7 bis 9):
//! * `DeviceCritical` eröffnet eine Meldung (höchstens eine offene je Gerät),
//! * `DeviceRecovered` schließt die offene Meldung,
//! * der Befehl `cmd.incidents.dismiss` weist eine Meldung ab.
//!
//! Nutzende zu informieren (E-Mail, F09) gehört zur Future Work und kommt als
//! weiterer Abonnent dieser Ereignisse dazu.

pub mod engine;
