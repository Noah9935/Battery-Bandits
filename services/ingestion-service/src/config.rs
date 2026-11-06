//! Konfiguration aus Umgebungsvariablen.

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub database_url: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_user: Option<String>,
    pub mqtt_password: Option<String>,
    pub mqtt_tls: bool,
    pub mqtt_topic_filter: String,
    pub mqtt_client_id: String,
    /// Herkunft der empfangenen Daten: `dozent` (Vorgabe) oder `simulation` (lokaler Simulator).
    pub data_source: shared::DataSource,
}

pub const DEFAULT_TOPIC_FILTER: &str = "traffic-nodes/+/measurements";

impl Config {
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Wie [`Config::from_env`], aber mit austauschbarer Quelle (für Tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let non_empty = |key: &str| get(key).filter(|v| !v.trim().is_empty());
        let mqtt_port: u16 = match non_empty("MQTT_PORT") {
            Some(p) => p
                .parse()
                .with_context(|| format!("MQTT_PORT ist keine Portnummer: {p}"))?,
            None => 1883,
        };
        let mqtt_tls = match non_empty("MQTT_TLS").as_deref() {
            Some("1" | "true" | "yes") => true,
            Some("0" | "false" | "no") => false,
            Some(other) => anyhow::bail!("MQTT_TLS muss true oder false sein, nicht {other}"),
            None => mqtt_port == 8883,
        };
        Ok(Self {
            database_url: non_empty("DATABASE_URL").context("DATABASE_URL fehlt")?,
            mqtt_host: non_empty("MQTT_HOST").context("MQTT_HOST fehlt")?,
            mqtt_port,
            mqtt_user: non_empty("MQTT_USER"),
            mqtt_password: non_empty("MQTT_PASSWORD"),
            mqtt_tls,
            mqtt_topic_filter: non_empty("MQTT_TOPIC_FILTER").unwrap_or_else(|| DEFAULT_TOPIC_FILTER.to_owned()),
            mqtt_client_id: non_empty("MQTT_CLIENT_ID").unwrap_or_else(|| "battery-bandits-ingestion".to_owned()),
            data_source: match non_empty("DATA_SOURCE") {
                Some(text) => text
                    .parse()
                    .map_err(|e| anyhow::anyhow!("DATA_SOURCE: {e} (erlaubt: dozent, simulation)"))?,
                None => shared::DataSource::Dozent,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn minimal_config_uses_real_topic_and_plain_mqtt() {
        let c = Config::from_lookup(lookup(&[("DATABASE_URL", "postgres://x"), ("MQTT_HOST", "broker")])).unwrap();
        assert_eq!(c.mqtt_port, 1883);
        assert!(!c.mqtt_tls);
        assert_eq!(c.mqtt_topic_filter, "traffic-nodes/+/measurements");
        assert_eq!(c.mqtt_user, None);
    }

    #[test]
    fn port_8883_turns_tls_on_unless_forced_off() {
        let base = [
            ("DATABASE_URL", "postgres://x"),
            ("MQTT_HOST", "mqtt.ti-labs.ai"),
            ("MQTT_PORT", "8883"),
        ];
        assert!(Config::from_lookup(lookup(&base)).unwrap().mqtt_tls);
        let forced = [base[0], base[1], base[2], ("MQTT_TLS", "false")];
        assert!(!Config::from_lookup(lookup(&forced)).unwrap().mqtt_tls);
    }

    #[test]
    fn empty_values_count_as_unset() {
        let c = Config::from_lookup(lookup(&[
            ("DATABASE_URL", "postgres://x"),
            ("MQTT_HOST", "b"),
            ("MQTT_USER", ""),
            ("MQTT_TOPIC_FILTER", " "),
        ]))
        .unwrap();
        assert_eq!(c.mqtt_user, None);
        assert_eq!(c.mqtt_topic_filter, DEFAULT_TOPIC_FILTER);
    }

    #[test]
    fn reports_missing_and_broken_values() {
        assert!(Config::from_lookup(lookup(&[("MQTT_HOST", "b")])).is_err());
        assert!(Config::from_lookup(lookup(&[("DATABASE_URL", "x")])).is_err());
        let bad_port = [("DATABASE_URL", "x"), ("MQTT_HOST", "b"), ("MQTT_PORT", "abc")];
        assert!(Config::from_lookup(lookup(&bad_port)).is_err());
        let bad_tls = [("DATABASE_URL", "x"), ("MQTT_HOST", "b"), ("MQTT_TLS", "vielleicht")];
        assert!(Config::from_lookup(lookup(&bad_tls)).is_err());
    }
}
