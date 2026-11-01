//! Observer-Muster und die Verarbeitung einer einzelnen Nachricht.
//!
//! Dieses Modul kennt weder MQTT noch die Datenbank, deshalb ist es ohne
//! Infrastruktur testbar (NF05).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use shared::Reading;
use shared::validation::{self, Rejection, Rules};

/// Beobachter eines neuen, gültigen Messwerts.
#[async_trait]
pub trait ReadingObserver: Send + Sync {
    fn name(&self) -> &'static str;

    /// Muss idempotent sein: Dieselbe Nachricht kann mehrfach ankommen
    /// (QoS 1) oder nach einem Fehler erneut verarbeitet werden.
    async fn on_reading(&self, reading: &Reading) -> Result<()>;
}

/// Senke für verworfene Nachrichten (F06).
#[async_trait]
pub trait RejectionSink: Send + Sync {
    async fn rejected(&self, topic: &str, payload: &[u8], reason: &Rejection) -> Result<()>;
}

/// Ergebnis der Verarbeitung einer Nachricht.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Accepted(Reading),
    Rejected(Rejection),
}

/// Das Subjekt: Prüft Nachrichten und benachrichtigt die Beobachter der Reihe nach.
///
/// Die Reihenfolge ist wichtig. Der Speicher kommt zuerst, damit nichts
/// gemeldet wird, was nicht gespeichert ist. Schlägt ein Beobachter fehl,
/// bricht die Kette ab und die Nachricht gilt als nicht verarbeitet.
pub struct Pipeline {
    observers: Vec<Arc<dyn ReadingObserver>>,
    rejections: Arc<dyn RejectionSink>,
    rules: Rules,
}

impl Pipeline {
    pub fn new(rejections: Arc<dyn RejectionSink>, rules: Rules) -> Self {
        Self {
            observers: Vec::new(),
            rejections,
            rules,
        }
    }

    pub fn attach(&mut self, observer: Arc<dyn ReadingObserver>) {
        self.observers.push(observer);
    }

    /// Verarbeitet eine Nachricht. `Err` bedeutet: nicht verarbeitet, später erneut versuchen.
    pub async fn handle(&self, topic: &str, payload: &[u8], now_ms: i64) -> Result<Outcome> {
        match validation::parse_message(topic, payload, now_ms, &self.rules) {
            Ok(reading) => {
                for observer in &self.observers {
                    observer
                        .on_reading(&reading)
                        .await
                        .map_err(|e| e.context(format!("Beobachter {}", observer.name())))?;
                }
                Ok(Outcome::Accepted(reading))
            }
            Err(reason) => {
                self.rejections.rejected(topic, payload, &reason).await?;
                Ok(Outcome::Rejected(reason))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    const NOW: i64 = 1_793_000_000_000;
    const TOPIC: &str = "trafficnode/TN-0001/battery";

    struct Recorder {
        label: &'static str,
        log: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    #[async_trait]
    impl ReadingObserver for Recorder {
        fn name(&self) -> &'static str {
            self.label
        }

        async fn on_reading(&self, reading: &Reading) -> Result<()> {
            if self.fail {
                anyhow::bail!("absichtlich kaputt");
            }
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:{}", self.label, reading.device_id));
            Ok(())
        }
    }

    #[derive(Default)]
    struct Sink {
        reasons: Mutex<Vec<String>>,
        calls: AtomicUsize,
    }

    #[async_trait]
    impl RejectionSink for Sink {
        async fn rejected(&self, _topic: &str, _payload: &[u8], reason: &Rejection) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reasons.lock().unwrap().push(reason.to_string());
            Ok(())
        }
    }

    fn pipeline(fail_second: bool) -> (Pipeline, Arc<Mutex<Vec<String>>>, Arc<Sink>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::new(Sink::default());
        let mut p = Pipeline::new(sink.clone(), Rules::default());
        p.attach(Arc::new(Recorder {
            label: "store",
            log: log.clone(),
            fail: false,
        }));
        p.attach(Arc::new(Recorder {
            label: "publish",
            log: log.clone(),
            fail: fail_second,
        }));
        (p, log, sink)
    }

    #[tokio::test]
    async fn valid_message_reaches_observers_in_order() {
        let (p, log, sink) = pipeline(false);
        let outcome = p.handle(TOPIC, br#"{"charge_pct":42}"#, NOW).await.unwrap();
        assert!(matches!(outcome, Outcome::Accepted(ref r) if r.charge_pct == 42.0));
        assert_eq!(*log.lock().unwrap(), vec!["store:TN-0001", "publish:TN-0001"]);
        assert_eq!(sink.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_message_is_logged_and_not_forwarded() {
        let (p, log, sink) = pipeline(false);
        let outcome = p.handle(TOPIC, br#"{"charge_pct":500}"#, NOW).await.unwrap();
        assert!(matches!(outcome, Outcome::Rejected(Rejection::ChargeOutOfRange(_))));
        assert!(log.lock().unwrap().is_empty());
        assert_eq!(sink.reasons.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn failing_observer_reports_error_and_stops_chain() {
        let (p, log, _sink) = pipeline(true);
        let err = p.handle(TOPIC, br#"{"charge_pct":42}"#, NOW).await.unwrap_err();
        assert!(format!("{err:#}").contains("publish"));
        assert_eq!(*log.lock().unwrap(), vec!["store:TN-0001"]);
    }

    #[tokio::test]
    async fn broken_json_is_rejected() {
        let (p, _log, sink) = pipeline(false);
        let outcome = p.handle(TOPIC, b"kein json", NOW).await.unwrap();
        assert!(matches!(outcome, Outcome::Rejected(Rejection::InvalidJson(_))));
        assert_eq!(sink.calls.load(Ordering::SeqCst), 1);
    }
}
