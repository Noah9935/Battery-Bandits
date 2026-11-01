//! Überwachungs-Service.
//!
//! Hört auf `ReadingStored`, bewertet den Ladestand gegen den Schwellwert
//! (Event Storming 4 und 6) und meldet `DeviceCritical` und `DeviceRecovered`.
//! Der Schwellwert ist ohne Codeänderung einstellbar (NF07), per Befehl
//! `cmd.monitor.set_threshold`; danach wird jedes Gerät neu bewertet.

pub mod engine;
