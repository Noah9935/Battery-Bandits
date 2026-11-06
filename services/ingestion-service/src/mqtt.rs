//! MQTT-Empfänger mit automatischem Wiederverbinden.
//!
//! * Persistente Sitzung (`clean_session = false`) und QoS 1: Der Broker hält Nachrichten
//!   für uns zurück, während wir weg sind.
//! * Manuelle Bestätigung: Eine Nachricht wird erst bestätigt, wenn sie verarbeitet ist.
//! * Nach einem Verbindungsfehler wartet die Schleife kurz und verbindet sich neu
//!   (Anforderung: spätestens nach 30 Sekunden wieder verbunden).

use std::future::Future;
use std::time::Duration;

use chrono::Utc;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport};

use crate::config::Config;
use crate::observer::{Dispatcher, Outcome};

const RECONNECT_DELAY: Duration = Duration::from_secs(2);
const PROCESS_ATTEMPTS: u32 = 3;

pub fn build_options(config: &Config) -> MqttOptions {
    let mut options = MqttOptions::new(&config.mqtt_client_id, &config.mqtt_host, config.mqtt_port);
    options.set_keep_alive(Duration::from_secs(20));
    options.set_clean_session(false);
    options.set_manual_acks(true);
    if let (Some(user), Some(password)) = (&config.mqtt_user, &config.mqtt_password) {
        options.set_credentials(user, password);
    }
    if config.mqtt_tls {
        install_crypto_provider();
        options.set_transport(Transport::tls_with_default_config());
    }
    options
}

/// rustls braucht einen ausdrücklich gewählten Krypto-Anbieter (sonst Absturz beim ersten TLS-Aufbau).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Empfängt Nachrichten, bis `shutdown` fertig wird.
pub async fn run(config: &Config, dispatcher: Dispatcher, shutdown: impl Future<Output = ()>) {
    let (client, mut eventloop) = AsyncClient::new(build_options(config), 256);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            event = eventloop.poll() => match event {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    tracing::info!(host = %config.mqtt_host, filter = %config.mqtt_topic_filter, "MQTT verbunden");
                    if let Err(e) = client.subscribe(&config.mqtt_topic_filter, QoS::AtLeastOnce).await {
                        tracing::error!("Abonnieren fehlgeschlagen: {e}");
                    }
                }
                Ok(Event::Incoming(Packet::Publish(publish))) => {
                    process(&dispatcher, &publish.topic, &publish.payload).await;
                    if let Err(e) = client.ack(&publish).await {
                        tracing::warn!("Bestätigung fehlgeschlagen: {e}");
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("MQTT-Verbindung gestört, neuer Versuch in {RECONNECT_DELAY:?}: {e}");
                    tokio::time::sleep(RECONNECT_DELAY).await;
                }
            }
        }
    }
    let _ = client.disconnect().await;
}

/// Verarbeitet eine Nachricht mit wenigen Wiederholungen, falls die Datenbank kurz weg ist.
async fn process(dispatcher: &Dispatcher, topic: &str, payload: &[u8]) {
    for attempt in 1..=PROCESS_ATTEMPTS {
        match dispatcher.handle(topic, payload, Utc::now()).await {
            Ok(Outcome::Valid) => return tracing::debug!(%topic, "Messwert gespeichert"),
            Ok(Outcome::Invalid) => return,
            Err(e) if attempt < PROCESS_ATTEMPTS => {
                tracing::warn!(%topic, attempt, "Verarbeitung fehlgeschlagen, neuer Versuch: {e:#}");
                tokio::time::sleep(Duration::from_secs(u64::from(attempt))).await;
            }
            Err(e) => tracing::error!(%topic, "Nachricht verloren, Verarbeitung endgültig fehlgeschlagen: {e:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tls: bool, user: Option<&str>) -> Config {
        Config {
            database_url: String::new(),
            mqtt_host: "mqtt.ti-labs.ai".into(),
            mqtt_port: if tls { 8883 } else { 1883 },
            mqtt_user: user.map(str::to_owned),
            mqtt_password: user.map(|_| "geheim".to_owned()),
            mqtt_tls: tls,
            mqtt_topic_filter: "traffic-nodes/+/measurements".into(),
            mqtt_client_id: "bb-test".into(),
            data_source: shared::DataSource::Dozent,
        }
    }

    #[test]
    fn options_use_persistent_session_and_manual_acks() {
        let o = build_options(&config(false, None));
        assert!(!o.clean_session());
        assert!(o.manual_acks());
        assert_eq!(o.client_id(), "bb-test");
        assert_eq!(o.broker_address(), ("mqtt.ti-labs.ai".to_owned(), 1883));
        assert!(o.credentials().is_none());
        assert!(matches!(o.transport(), Transport::Tcp));
    }

    #[test]
    fn options_enable_tls_and_credentials() {
        let o = build_options(&config(true, Some("gruppe")));
        assert!(matches!(o.transport(), Transport::Tls(_)));
        assert_eq!(o.credentials().map(|login| login.username), Some("gruppe".to_owned()));
    }
}
