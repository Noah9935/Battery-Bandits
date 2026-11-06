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

/// Wie [`connect_and_migrate`], versucht es beim Start aber bis zu eine Minute lang erneut,
/// weil die Datenbank gerade erst hochfahren kann.
pub async fn connect_with_retry(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let mut attempt = 1;
    loop {
        match connect_and_migrate(database_url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempt >= 30 => return Err(e),
            Err(_) => {
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

/// Sendet ein Ereignis auf dem Kanal `domain_events`. Innerhalb einer Transaktion
/// aufgerufen, wird es erst beim Commit zugestellt.
pub async fn notify_event<'e, E>(executor: E, event: &crate::DomainEvent) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("select pg_notify($1, $2)")
        .bind(crate::events::CHANNEL)
        .bind(event.to_json())
        .execute(executor)
        .await?;
    Ok(())
}
