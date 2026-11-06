//! Datenbankzugang und Migrationen (nur mit Feature `sqlx`).

use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;

/// Eingebettete Migrationen aus `migrations/` im Workspace-Wurzelverzeichnis.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Verbindet sich mit der Datenbank und wendet alle offenen Migrationen an.
///
/// sqlx sperrt dabei per Advisory Lock, mehrere Dienste dürfen also gleichzeitig starten.
pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new().max_connections(10).connect(database_url).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
