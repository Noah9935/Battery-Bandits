use std::sync::Arc;

use anyhow::Result;
use kit::bus::Bus;
use kit::env;
use monitor::engine::Monitor;
use shared::events::{SetThreshold, subject};

#[tokio::main]
async fn main() -> Result<()> {
    kit::init_tracing();
    let database_url = env::required("DATABASE_URL")?;
    let nats_url = env::or("NATS_URL", "nats://nats:4222");
    let default_threshold: f64 = env::parse_or("CRITICAL_THRESHOLD_PCT", 20.0)?;

    let pool = kit::db::connect_owned(&database_url, "monitor").await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let bus = Bus::connect(&nats_url).await?;

    let monitor = Arc::new(Monitor::new(pool, bus.clone(), default_threshold).await?);
    tracing::info!(threshold = monitor.threshold().await?, "Überwachung bereit");

    let readings = {
        let monitor = monitor.clone();
        bus.consume("monitor", &[subject::READING_STORED], move |subject, payload| {
            let monitor = monitor.clone();
            async move { monitor.on_event(&subject, &payload).await }
        })
    };
    let commands = {
        let monitor = monitor.clone();
        bus.serve_commands(subject::CMD_SET_THRESHOLD, move |cmd: SetThreshold| {
            let monitor = monitor.clone();
            async move { monitor.handle_set_threshold(cmd).await }
        })
    };

    tokio::select! {
        r = readings => r,
        r = commands => r,
        _ = kit::shutdown::signal() => Ok(()),
    }
}
