//! Spielt historische Nachrichten aus stdin in die Datenbank ein (siehe `ingestion_service::import`).
//!
//!   <Datensätze als JSON-Zeilen> | DATABASE_URL=... import-history

use std::sync::Arc;

use anyhow::{Context, Result};
use ingestion_service::import::import_lines;
use ingestion_service::observer::Dispatcher;
use ingestion_service::store::PostgresObserver;

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL fehlt")?;
    let pool = shared::db::connect_with_retry(&database_url)
        .await
        .context("Datenbank nicht erreichbar")?;
    // DATA_SOURCE=dozent (Vorgabe) für echte Daten, simulation für erfundene.
    let source = match std::env::var("DATA_SOURCE").ok().filter(|v| !v.trim().is_empty()) {
        Some(text) => text.parse().map_err(|e| anyhow::anyhow!("DATA_SOURCE: {e}"))?,
        None => shared::DataSource::Dozent,
    };
    let mut dispatcher = Dispatcher::new();
    dispatcher.subscribe(Arc::new(PostgresObserver::new(pool).with_source(source)));

    let stats = import_lines(&dispatcher, std::io::stdin().lock()).await?;
    println!(
        "{} Nachrichten gespeichert, {} verworfen und protokolliert",
        stats.valid, stats.invalid
    );
    Ok(())
}
