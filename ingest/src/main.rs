use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ingest::config::Config;
use ingest::observer::Pipeline;
use ingest::receiver::MqttReceiver;
use ingest::store::{EventPublisher, ReadingStore, RejectionStore};
use kit::bus::Bus;
use shared::validation::Rules;

#[tokio::main]
async fn main() -> Result<()> {
    kit::init_tracing();
    let config = Config::from_env()?;

    let pool = kit::db::connect_owned(&config.database_url, "ingest").await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("Datenbank bereit");

    let bus = Bus::connect(&config.nats_url).await?;
    tracing::info!("NATS verbunden");

    let rejections = Arc::new(RejectionStore::new(pool.clone()));

    // Beobachter in fester Reihenfolge: erst speichern, dann melden.
    let mut pipeline = Pipeline::new(rejections.clone(), Rules::default());
    pipeline.attach(Arc::new(ReadingStore::new(pool.clone())));
    pipeline.attach(Arc::new(EventPublisher::new(bus)));

    let days = config.rejected_retention_days;
    tokio::spawn(async move {
        loop {
            match rejections.purge_older_than_days(days).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("{n} alte verworfene Nachrichten gelöscht"),
                Err(e) => tracing::warn!("Aufräumen fehlgeschlagen: {e:#}"),
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    MqttReceiver::new(config.mqtt, Arc::new(pipeline))
        .run(kit::shutdown::signal())
        .await
}
