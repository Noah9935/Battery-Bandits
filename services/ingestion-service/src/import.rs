//! Import historischer Nachrichten (zum Beispiel aus der Messdatenplattform) durch denselben Weg wie live
//! empfangene: Prüfung, Verwerfen und Protokollieren, Ladezustand, Speichern. Als Empfangszeitpunkt gilt
//! der Originalzeitpunkt, damit die Stromintegration die echten Abstände sieht.
//!
//! Eingabe: eine JSON-Zeile je Nachricht `{"t": "<RFC 3339>", "topic": "...", "payload": {...}}`,
//! zeitlich aufsteigend sortiert.

use std::io::BufRead;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::observer::{Dispatcher, Outcome};

#[derive(Debug, Deserialize, PartialEq)]
pub struct Record {
    pub t: DateTime<Utc>,
    pub topic: String,
    pub payload: serde_json::Value,
}

impl Record {
    /// Nutzlast als Bytes, wie sie der Broker geliefert hätte (Text bleibt Text, JSON wird serialisiert).
    pub fn payload_bytes(&self) -> Vec<u8> {
        match &self.payload {
            serde_json::Value::String(text) => text.clone().into_bytes(),
            other => serde_json::to_vec(other).unwrap_or_default(),
        }
    }
}

pub fn parse_line(line: &str) -> Result<Record> {
    serde_json::from_str(line).context("Zeile ist kein gültiger Datensatz (t, topic, payload)")
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub valid: usize,
    pub invalid: usize,
}

/// Liest Zeilen bis zum Ende und verarbeitet sie der Reihe nach. Leere Zeilen werden übersprungen.
pub async fn import_lines(dispatcher: &Dispatcher, reader: impl BufRead) -> Result<Stats> {
    let mut stats = Stats::default();
    for (number, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record = parse_line(&line).with_context(|| format!("Zeile {}", number + 1))?;
        match dispatcher
            .handle(&record.topic, &record.payload_bytes(), record.t)
            .await?
        {
            Outcome::Valid => stats.valid += 1,
            Outcome::Invalid => stats.invalid += 1,
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_record_with_json_payload() {
        let r = parse_line(r#"{"t":"2026-11-05T15:04:39.5Z","topic":"traffic-nodes/TN0001/measurements","payload":{"voltage_mV":12296.25}}"#).unwrap();
        assert_eq!(r.topic, "traffic-nodes/TN0001/measurements");
        assert_eq!(r.t.to_rfc3339(), "2026-11-05T15:04:39.500+00:00");
        assert_eq!(r.payload_bytes(), br#"{"voltage_mV":12296.25}"#);
    }

    #[test]
    fn text_payloads_stay_text() {
        let r = parse_line(r#"{"t":"2026-11-05T15:04:39Z","topic":"a/b/c","payload":"kein json {"}"#).unwrap();
        assert_eq!(r.payload_bytes(), b"kein json {");
    }

    #[test]
    fn rejects_lines_without_the_required_fields() {
        assert!(parse_line("{}").is_err());
        assert!(parse_line("kein json").is_err());
        assert!(parse_line(r#"{"t":"gestern","topic":"a","payload":{}}"#).is_err());
    }
}
