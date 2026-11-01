use anyhow::Result;
use kit::env;

/// Konfiguration des Empfangsdienstes aus Umgebungsvariablen.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub nats_url: String,
    pub mqtt: MqttConfig,
    /// Wie lange verworfene Nachrichten aufgehoben werden
    pub rejected_retention_days: i32,
}

#[derive(Debug, Clone)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: bool,
    pub topic: String,
    /// Feste Kennung: Mit ihr hält der Broker die Sitzung und die ungelesenen Nachrichten vor.
    pub client_id: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let port: u16 = env::parse_or("MQTT_PORT", 1883)?;
        let username = std::env::var("MQTT_USER").ok().filter(|v| !v.is_empty());
        let password = std::env::var("MQTT_PASSWORD").ok().filter(|v| !v.is_empty());
        Ok(Self {
            database_url: env::required("DATABASE_URL")?,
            nats_url: env::or("NATS_URL", "nats://nats:4222"),
            rejected_retention_days: env::parse_or("REJECTED_RETENTION_DAYS", 30)?,
            mqtt: MqttConfig {
                host: env::required("MQTT_HOST")?,
                port,
                username,
                password,
                tls: env::flag("MQTT_TLS", port == 8883),
                topic: env::or("MQTT_TOPIC", "trafficnode/+/battery"),
                client_id: env::or("MQTT_CLIENT_ID", "battery-bandits-ingest"),
            },
        })
    }
}
