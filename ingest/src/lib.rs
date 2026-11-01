//! MQTT-Empfangsdienst (Erfassungs-Service).
//!
//! Ablauf je Nachricht (Event Storming 1 bis 3): empfangen, prüfen, dann
//! entweder als Messwert speichern und melden oder verwerfen und protokollieren.
//!
//! Das Observer-Muster ist vorgegeben: Der [`receiver::MqttReceiver`] ist das
//! Subjekt, [`store::ReadingStore`] und [`store::EventPublisher`] sind
//! Beobachter. Weitere Dienste (Überwachung, Meldungen, Live-Anzeige) sind
//! Beobachter im Netz: Sie hören auf die Ereignisse, die der Publisher
//! veröffentlicht.

pub mod config;
pub mod observer;
pub mod receiver;
pub mod store;
