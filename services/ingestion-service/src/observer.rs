//! Observer-Muster: Der MQTT-Empfänger meldet Ergebnisse an beliebig viele Beobachter.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::classify::{Incoming, InvalidMessage, ValidReading, classify};

#[async_trait]
pub trait ReadingObserver: Send + Sync {
    /// Eine gültige Messung ist eingetroffen.
    async fn on_valid(&self, reading: &ValidReading, received_at: DateTime<Utc>) -> Result<()>;
    /// Eine Nachricht wurde verworfen (zum Protokollieren).
    async fn on_invalid(&self, message: &InvalidMessage, received_at: DateTime<Utc>) -> Result<()>;
}

/// Was mit einer Nachricht passiert ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Valid,
    Invalid,
}

#[derive(Default, Clone)]
pub struct Dispatcher {
    observers: Vec<Arc<dyn ReadingObserver>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&mut self, observer: Arc<dyn ReadingObserver>) {
        self.observers.push(observer);
    }

    /// Prüft die Nachricht und verteilt sie an alle Beobachter.
    ///
    /// Fällt ein Beobachter aus, bekommen die übrigen die Nachricht trotzdem; der erste
    /// Fehler wird zurückgegeben, damit der Aufrufer es erneut versuchen kann.
    pub async fn handle(&self, topic: &str, payload: &[u8], received_at: DateTime<Utc>) -> Result<Outcome> {
        let incoming = classify(topic, payload);
        let mut first_error = None;
        for observer in &self.observers {
            let result = match &incoming {
                Incoming::Valid(reading) => observer.on_valid(reading, received_at).await,
                Incoming::Invalid(message) => observer.on_invalid(message, received_at).await,
            };
            if let Err(e) = result {
                first_error.get_or_insert(e);
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(match incoming {
                Incoming::Valid(_) => Outcome::Valid,
                Incoming::Invalid(_) => Outcome::Invalid,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder {
        valid: Mutex<Vec<String>>,
        invalid: Mutex<Vec<String>>,
        fail: bool,
    }

    #[async_trait]
    impl ReadingObserver for Recorder {
        async fn on_valid(&self, reading: &ValidReading, _: DateTime<Utc>) -> Result<()> {
            self.valid.lock().unwrap().push(reading.node_id.clone());
            if self.fail {
                anyhow::bail!("Datenbank weg")
            }
            Ok(())
        }
        async fn on_invalid(&self, message: &InvalidMessage, _: DateTime<Utc>) -> Result<()> {
            self.invalid.lock().unwrap().push(message.reason.clone());
            Ok(())
        }
    }

    const GOOD: &[u8] = br#"{"current_mA":-300,"voltage_mV":12300,"temperature_C":15}"#;

    #[tokio::test]
    async fn routes_valid_messages_to_every_observer() {
        let (a, b) = (Arc::new(Recorder::default()), Arc::new(Recorder::default()));
        let mut d = Dispatcher::new();
        d.subscribe(a.clone());
        d.subscribe(b.clone());

        let outcome = d
            .handle("traffic-nodes/TN0007/measurements", GOOD, Utc::now())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Valid);
        assert_eq!(*a.valid.lock().unwrap(), ["TN0007"]);
        assert_eq!(*b.valid.lock().unwrap(), ["TN0007"]);
        assert!(a.invalid.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn routes_invalid_messages_as_invalid() {
        let a = Arc::new(Recorder::default());
        let mut d = Dispatcher::new();
        d.subscribe(a.clone());

        let outcome = d
            .handle("traffic-nodes/TN0007/measurements", b"mist", Utc::now())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Invalid);
        assert_eq!(a.invalid.lock().unwrap().len(), 1);
        assert!(a.valid.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failing_observer_does_not_block_the_others() {
        let bad = Arc::new(Recorder {
            fail: true,
            ..Default::default()
        });
        let good = Arc::new(Recorder::default());
        let mut d = Dispatcher::new();
        d.subscribe(bad);
        d.subscribe(good.clone());

        let result = d.handle("traffic-nodes/TN0001/measurements", GOOD, Utc::now()).await;

        assert!(result.is_err());
        assert_eq!(good.valid.lock().unwrap().len(), 1);
    }
}
