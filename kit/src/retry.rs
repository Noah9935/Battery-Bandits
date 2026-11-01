//! Wiederholen beim Start: Datenbank und NATS sind beim Hochfahren oft noch nicht bereit.

use std::fmt::Display;
use std::future::Future;
use std::time::Duration;

use anyhow::{Result, anyhow};

/// Führt `op` bis zu `attempts` Mal aus und wartet dazwischen zwei Sekunden.
pub async fn retry<T, E, F, Fut>(what: &str, attempts: u32, mut op: F) -> Result<T>
where
    E: Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut last = String::new();
    for attempt in 1..=attempts {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e.to_string();
                tracing::warn!("{what} nicht erreichbar (Versuch {attempt} von {attempts}): {last}");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    Err(anyhow!("{what} nach {attempts} Versuchen nicht erreichbar: {last}"))
}
