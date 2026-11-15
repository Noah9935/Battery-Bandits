//! Deckel für die Datenbank: Wird der vorgesehene Platz (Vorgabe 10 GB) zu 95 Prozent voll, löscht dieser Dienst die
//! ältesten Messwerte, ein Datenstück (Chunk, etwa eine Woche) nach dem anderen, bis es wieder passt. Das jüngste
//! Datenstück bleibt immer, damit nie alles auf einmal verschwindet. Zusätzlich gilt die Aufbewahrung von 12 Monaten,
//! die die Datenbank selbst durchsetzt.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// Löscht die ältesten Messwerte, solange die Datenbank über der Schwelle (95 Prozent von `quota_bytes`) liegt.
/// Gibt die Zahl der gelöschten Datenstücke zurück.
pub async fn enforce_quota(pool: &PgPool, quota_bytes: i64) -> Result<usize> {
    let trigger = shared::efficiency::prune_trigger_bytes(quota_bytes);
    let mut dropped = 0;
    loop {
        let size: i64 = sqlx::query_scalar("select pg_database_size(current_database())")
            .fetch_one(pool)
            .await?;
        if size <= trigger {
            break;
        }
        let chunks: i64 = sqlx::query_scalar(
            "select count(*) from timescaledb_information.chunks where hypertable_name = 'readings'",
        )
        .fetch_one(pool)
        .await?;
        if chunks <= 1 {
            tracing::warn!(
                size,
                quota_bytes,
                "Datenbank über dem Platz, aber es bleibt nur noch ein Datenstück"
            );
            break;
        }
        let oldest_end: Option<DateTime<Utc>> = sqlx::query_scalar(
            "select range_end from timescaledb_information.chunks \
             where hypertable_name = 'readings' order by range_start limit 1",
        )
        .fetch_optional(pool)
        .await?;
        let Some(oldest_end) = oldest_end else { break };
        let gone: Vec<String> = sqlx::query_scalar("select drop_chunks('readings', older_than => $1)::text")
            .bind(oldest_end)
            .fetch_all(pool)
            .await?;
        if gone.is_empty() {
            break;
        }
        dropped += gone.len();
    }
    Ok(dropped)
}

/// Prüft den Platz regelmäßig (der erste Lauf gleich beim Start). Fehler werden gemeldet, der Dienst läuft weiter.
pub async fn run(pool: PgPool, quota_bytes: i64, every: Duration) {
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        match enforce_quota(&pool, quota_bytes).await {
            Ok(0) => {}
            Ok(n) => tracing::warn!(
                chunks = n,
                quota_bytes,
                "Datenbank über 95 Prozent des Platzes: die ältesten Messwerte wurden gelöscht"
            ),
            Err(e) => tracing::warn!("Platz der Datenbank nicht geprüft: {e:#}"),
        }
    }
}
