//! Der MQTT-Empfänger (F01).
//!
//! * QoS 1 und eine Sitzung mit fester Kennung: Was während eines Neustarts
//!   eintrifft, hält der Broker vor (NF02).
//! * Nachrichten werden erst bestätigt, wenn sie verarbeitet sind. Stürzt der
//!   Dienst vorher ab, stellt der Broker sie erneut zu.
//! * Reißt die Verbindung ab, verbindet sich der Empfänger selbst neu, mit
//!   Wartezeit bis höchstens zehn Sekunden (NF03: Ziel höchstens 30 Sekunden).

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, Publish, QoS, Transport};
use tokio::sync::mpsc;

use crate::config::MqttConfig;
use crate::observer::{Outcome, Pipeline};

const MAX_BACKOFF: Duration = Duration::from_secs(10);
const MAX_ATTEMPTS: u32 = 5;

pub struct MqttReceiver {
    config: MqttConfig,
    pipeline: Arc<Pipeline>,
}

impl MqttReceiver {
    pub fn new(config: MqttConfig, pipeline: Arc<Pipeline>) -> Self {
        Self { config, pipeline }
    }

    fn options(&self) -> MqttOptions {
        let c = &self.config;
        let mut options = MqttOptions::new(&c.client_id, &c.host, c.port);
        options
            .set_keep_alive(Duration::from_secs(30))
            .set_clean_session(false)
            .set_manual_acks(true);
        if let (Some(user), Some(password)) = (&c.username, &c.password) {
            options.set_credentials(user, password);
        }
        if c.tls {
            options.set_transport(Transport::tls_with_default_config());
        }
        options
    }

    /// Läuft, bis `shutdown` fertig ist.
    pub async fn run(self, shutdown: impl Future<Output = ()>) -> Result<()> {
        let (client, mut eventloop) = AsyncClient::new(self.options(), 256);
        let (tx, rx) = mpsc::channel::<Publish>(1024);
        let worker = tokio::spawn(process_loop(client.clone(), self.pipeline.clone(), rx));

        tracing::info!(
            host = %self.config.host, port = self.config.port, tls = self.config.tls,
            topic = %self.config.topic, "Verbinde mit dem MQTT-Broker"
        );

        tokio::pin!(shutdown);
        let mut backoff = Duration::from_secs(1);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                event = eventloop.poll() => match event {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        backoff = Duration::from_secs(1);
                        tracing::info!("Mit dem MQTT-Broker verbunden");
                        // Bei jeder Verbindung neu abonnieren, auch wenn der Broker die Sitzung hält.
                        if let Err(e) = client.try_subscribe(&self.config.topic, QoS::AtLeastOnce) {
                            tracing::error!("Abonnieren fehlgeschlagen: {e}");
                        }
                    }
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        if tx.send(publish).await.is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("MQTT-Verbindung gestört: {e}. Neuer Versuch in {} s", backoff.as_secs());
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                },
            }
        }

        // Geordnet beenden: Warteschlange leeren lassen, dann trennen.
        drop(tx);
        let _ = worker.await;
        let _ = client.disconnect().await;
        Ok(())
    }
}

/// Arbeitet die empfangenen Nachrichten nacheinander ab und bestätigt sie danach.
async fn process_loop(client: AsyncClient, pipeline: Arc<Pipeline>, mut rx: mpsc::Receiver<Publish>) {
    while let Some(publish) = rx.recv().await {
        let mut attempt = 0;
        loop {
            match pipeline
                .handle(&publish.topic, &publish.payload, kit::time::now_ms())
                .await
            {
                Ok(outcome) => {
                    match &outcome {
                        Outcome::Accepted(r) => {
                            tracing::debug!(device = %r.device_id, charge = r.charge_pct, "Messwert gespeichert");
                        }
                        Outcome::Rejected(reason) => tracing::debug!(%reason, "Nachricht verworfen"),
                    }
                    if let Err(e) = client.ack(&publish).await {
                        tracing::warn!("Bestätigung an den Broker fehlgeschlagen: {e}");
                    }
                    break;
                }
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_ATTEMPTS {
                        // Nicht bestätigen: Der Broker stellt die Nachricht bei der nächsten Verbindung erneut zu.
                        tracing::error!(topic = %publish.topic, "Nachricht nach {attempt} Versuchen nicht verarbeitet: {e:#}");
                        break;
                    }
                    let wait = Duration::from_secs(1 << attempt).min(MAX_BACKOFF);
                    tracing::warn!(
                        "Verarbeitung fehlgeschlagen (Versuch {attempt}): {e:#}. Wiederholung in {} s",
                        wait.as_secs()
                    );
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }
}
