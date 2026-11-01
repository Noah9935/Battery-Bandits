//! Datenbankzugang. Jeder Dienst besitzt ein eigenes Schema in derselben
//! TimescaleDB-Instanz und migriert nur dieses.

use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::retry::retry;

/// Verbindet mit der Datenbank und legt das Schema des Dienstes an.
///
/// Der Suchpfad der Verbindungen zeigt zuerst auf das eigene Schema, dort liegt
/// auch die Tabelle `_sqlx_migrations`. `public` bleibt im Pfad, weil die
/// TimescaleDB-Funktionen dort wohnen. Abfragen nennen ihre Tabellen trotzdem
/// immer mit Schema, damit sie eindeutig sind.
pub async fn connect_owned(url: &str, schema: &'static str) -> Result<PgPool> {
    let admin = retry("Datenbank", 60, || PgPoolOptions::new().max_connections(1).connect(url)).await?;
    sqlx::query(&format!("create schema if not exists {schema}"))
        .execute(&admin)
        .await
        .with_context(|| format!("Schema {schema} konnte nicht angelegt werden"))?;
    admin.close().await;

    PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                sqlx::query(&format!("set search_path to {schema}, public"))
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await
        .context("Verbindung zur Datenbank fehlgeschlagen")
}

/// Verbindung ohne eigenes Schema, für die lesende API.
pub async fn connect_shared(url: &str) -> Result<PgPool> {
    retry("Datenbank", 60, || PgPoolOptions::new().max_connections(8).connect(url)).await
}

/// Wartet, bis die genannten Tabellen (`schema.tabelle`) existieren, also die
/// besitzenden Dienste ihre Migrationen abgeschlossen haben.
pub async fn wait_for_tables(pool: &PgPool, tables: &[&str]) -> Result<()> {
    for table in tables {
        retry("Tabelle", 90, || async {
            let found: Option<String> = sqlx::query_scalar("select to_regclass($1)::text")
                .bind(table)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            found.map(|_| ()).ok_or_else(|| format!("{table} existiert noch nicht"))
        })
        .await?;
    }
    Ok(())
}
