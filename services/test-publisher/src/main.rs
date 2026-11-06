use std::time::Duration;

use anyhow::{Result, bail};
use rumqttc::{AsyncClient, MqttOptions, QoS, Transport};
use test_publisher::{Fleet, FleetConfig};
use tracing_subscriber::EnvFilter;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T> {
    match env(key) {
        Some(v) => v
            .parse()
            .map_err(|_| anyhow::anyhow!("{key} hat einen ungültigen Wert: {v}")),
        None => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let host = env("MQTT_HOST").ok_or_else(|| anyhow::anyhow!("MQTT_HOST fehlt"))?;
    let port: u16 = parse_env("MQTT_PORT", 1883)?;

    // Schutz: Erfundene Messwerte gehören nie in den gemeinsamen Broker der Vorlesung.
    let local = matches!(
        host.as_str(),
        "localhost" | "127.0.0.1" | "mosquitto" | "host.docker.internal"
    );
    if !local && env("SIM_ALLOW_REMOTE").as_deref() != Some("true") {
        bail!(
            "Der Test-Publisher sendet nur an lokale Broker. Für {host} SIM_ALLOW_REMOTE=true setzen (nicht für mqtt.ti-labs.ai)."
        );
    }

    let defaults = FleetConfig::default();
    let config = FleetConfig {
        seed: parse_env("SIM_SEED", defaults.seed)?,
        speedup: parse_env("SIM_SPEEDUP", defaults.speedup)?,
        invalid_rate: parse_env("SIM_INVALID_RATE", defaults.invalid_rate)?,
        interval_secs: parse_env("SIM_INTERVAL_SECS", defaults.interval_secs)?,
        topic_template: env("SIM_TOPIC_TEMPLATE").unwrap_or(defaults.topic_template),
        ..defaults
    };
    let interval = Duration::from_secs_f64(config.interval_secs.max(0.5));

    let mut options = MqttOptions::new(
        env("MQTT_CLIENT_ID").unwrap_or_else(|| "battery-bandits-test-publisher".into()),
        &host,
        port,
    );
    options.set_keep_alive(Duration::from_secs(20));
    if let (Some(user), Some(password)) = (env("MQTT_USER"), env("MQTT_PASSWORD")) {
        options.set_credentials(user, password);
    }
    if parse_env("MQTT_TLS", port == 8883)? {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        options.set_transport(Transport::tls_with_default_config());
    }
    let (client, mut eventloop) = AsyncClient::new(options, 256);
    tokio::spawn(async move {
        loop {
            if let Err(e) = eventloop.poll().await {
                tracing::warn!("MQTT-Verbindung gestört: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });

    let mut fleet = Fleet::new(config.clone());
    tracing::info!(nodes = fleet.node_count(), ?interval, speedup = config.speedup, %host, "Test-Publisher gestartet");

    let mut ticker = tokio::time::interval(interval);
    let mut round = 0u64;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = ticker.tick() => {
                round += 1;
                let messages = fleet.tick();
                let invalid = messages.iter().filter(|m| !m.valid).count();
                for m in &messages {
                    if let Err(e) = client.publish(&m.topic, QoS::AtLeastOnce, false, m.payload.clone()).await {
                        tracing::warn!("Senden fehlgeschlagen: {e}");
                    }
                }
                tracing::info!(round, sent = messages.len(), invalid, "Runde gesendet");
            }
        }
    }
    let _ = client.disconnect().await;
    Ok(())
}
