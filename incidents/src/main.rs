use std::sync::Arc;

use anyhow::Result;
use incidents::engine::Incidents;
use kit::bus::Bus;
use kit::env;
use shared::events::{DismissIncident, subject};

#[tokio::main]
async fn main() -> Result<()> {
    kit::init_tracing();
    let database_url = env::required("DATABASE_URL")?;
    let nats_url = env::or("NATS_URL", "nats://nats:4222");

    let pool = kit::db::connect_owned(&database_url, "incidents").await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let bus = Bus::connect(&nats_url).await?;

    let incidents = Arc::new(Incidents::new(pool, bus.clone()));
    tracing::info!("Meldungsdienst bereit");

    let events = {
        let incidents = incidents.clone();
        bus.consume(
            "incidents",
            &[subject::DEVICE_CRITICAL, subject::DEVICE_RECOVERED],
            move |subject, payload| {
                let incidents = incidents.clone();
                async move { incidents.on_event(&subject, &payload).await }
            },
        )
    };
    let commands = {
        let incidents = incidents.clone();
        bus.serve_commands(subject::CMD_DISMISS_INCIDENT, move |cmd: DismissIncident| {
            let incidents = incidents.clone();
            async move { incidents.handle_dismiss(cmd).await }
        })
    };

    tokio::select! {
        r = events => r,
        r = commands => r,
        _ = kit::shutdown::signal() => Ok(()),
    }
}
