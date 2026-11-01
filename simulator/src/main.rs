use std::time::Duration;

use anyhow::Result;
use kit::env;
use rumqttc::{AsyncClient, MqttOptions, QoS, Transport};
use simulator::Fleet;

#[tokio::main]
async fn main() -> Result<()> {
    kit::init_tracing();

    let host = env::required("MQTT_HOST")?;
    let port: u16 = env::parse_or("MQTT_PORT", 1883)?;
    let devices: usize = env::parse_or("SIM_DEVICES", 500)?;
    let interval_secs: u64 = env::parse_or("SIM_INTERVAL_SECS", 300)?;
    let time_scale: f64 = env::parse_or("SIM_TIME_SCALE", 1.0)?;
    let invalid_rate: f64 = env::parse_or("SIM_INVALID_RATE", 0.01)?;
    let seed: u64 = env::parse_or("SIM_SEED", 42)?;

    let mut options = MqttOptions::new(env::or("MQTT_CLIENT_ID", "battery-bandits-simulator"), &host, port);
    options.set_keep_alive(Duration::from_secs(30));
    if let (Ok(user), Ok(password)) = (std::env::var("MQTT_USER"), std::env::var("MQTT_PASSWORD"))
        && !user.is_empty()
    {
        options.set_credentials(user, password);
    }
    if env::flag("MQTT_TLS", port == 8883) {
        options.set_transport(Transport::tls_with_default_config());
    }
    let (client, mut eventloop) = AsyncClient::new(options, 1024);

    // Die Ereignisschleife muss laufen, sonst werden keine Nachrichten gesendet.
    tokio::spawn(async move {
        loop {
            if let Err(e) = eventloop.poll().await {
                tracing::warn!("MQTT-Verbindung gestört: {e}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });

    let mut fleet = Fleet::new(devices, seed, invalid_rate, time_scale);
    tracing::info!(devices, interval_secs, time_scale, invalid_rate, %host, port, "Simulator gestartet");

    // Die Geräte senden verteilt über das Intervall, nicht alle im selben Moment.
    let spacing = Duration::from_millis(((interval_secs * 1000) / devices.max(1) as u64).min(50));
    let mut last = kit::time::now_ms();
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
    let shutdown = kit::shutdown::signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            _ = ticker.tick() => {
                let now = kit::time::now_ms();
                let messages = fleet.step(now, now - last);
                last = now;
                let invalid = messages.iter().filter(|m| !m.valid).count();
                for m in messages {
                    if let Err(e) = client.publish(m.topic, QoS::AtLeastOnce, false, m.payload).await {
                        tracing::warn!("Senden fehlgeschlagen: {e}");
                    }
                    tokio::time::sleep(spacing).await;
                }
                tracing::info!(sent = fleet.len(), invalid, "Runde gesendet");
            }
        }
    }
    let _ = client.disconnect().await;
    Ok(())
}
