use anyhow::{Context, Result};
use dashboard_service::{AppState, api, sse};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL fehlt")?;
    let operations_url =
        std::env::var("OPERATIONS_URL").unwrap_or_else(|_| "http://operations-service:8082".to_owned());
    let addr = std::env::var("DASHBOARD_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());

    let pool = shared::db::connect_with_retry(&database_url)
        .await
        .context("Datenbank nicht erreichbar")?;
    let auth_required = std::env::var("AUTH_REQUIRED")
        .map(|v| v != "false" && v != "0")
        .unwrap_or(true);
    let cookie_secure = std::env::var("COOKIE_SECURE").is_ok_and(|v| v == "true" || v == "1");
    let users = dashboard_service::auth::parse_users(&std::env::var("BB_USERS").unwrap_or_default())
        .map_err(|e| anyhow::anyhow!("BB_USERS: {e}"))?;
    dashboard_service::auth::upsert_users(&pool, &users).await?;
    if auth_required {
        let existing: i64 = sqlx::query_scalar("select count(*) from users")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            existing > 0,
            "AUTH_REQUIRED ist an, aber es gibt keine Konten. BB_USERS setzen (name:passwort:Anzeigename;...)"
        );
        tracing::info!(konten = existing, "Anmeldung ist Pflicht");
    } else {
        tracing::warn!("Anmeldung ist ABGESCHALTET, alle Anfragen gelten als Gast");
    }
    let state = AppState::new(pool.clone(), operations_url)
        .with_auth(auth_required)
        .with_cookie_secure(cookie_secure);
    sse::spawn_pump(pool, state.bus.clone());

    let tcp = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("{addr} nicht verfügbar"))?;
    tracing::info!(%addr, "Dashboard-Service läuft");
    axum::serve(tcp, api::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Beende auf Anforderung");
        })
        .await
        .context("HTTP-Server beendet")
}
