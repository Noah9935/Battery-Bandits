//! Datenbankoperationen. Ereignisse werden in derselben Transaktion gesendet und
//! erreichen die Empfänger erst nach dem Commit.

use chrono::Utc;
use shared::db::notify_event;
use shared::{Alert, AlertKind, AlertStatus, DomainEvent, MaintenanceStatus, MaintenanceTask, NewMaintenanceTask};
use sqlx::{PgConnection, PgPool};

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("{0} nicht gefunden")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("Datenbankfehler: {0}")]
    Db(#[from] sqlx::Error),
}

const ALERT_COLUMNS: &str = "id, device_id, kind, status, created_at, closed_at";
const TASK_COLUMNS: &str = "id, device_id, alert_id, status, note, created_at, completed_at";

/// Eröffnet eine Meldung, falls das Gerät keine offene hat. Gibt die neue Meldung zurück.
pub async fn open_alert(conn: &mut PgConnection, device_id: &str, kind: AlertKind) -> Result<Option<Alert>, RepoError> {
    let alert: Option<Alert> = sqlx::query_as(&format!(
        "insert into alerts (device_id, kind) values ($1, $2) \
         on conflict (device_id) where status = 'open' do nothing returning {ALERT_COLUMNS}"
    ))
    .bind(device_id)
    .bind(kind)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(alert) = &alert {
        let event = DomainEvent::AlertCreated {
            alert_id: alert.id,
            device_id: device_id.to_owned(),
            kind,
        };
        notify_event(&mut *conn, &event).await?;
    }
    Ok(alert)
}

/// Schließt alle offenen Meldungen des Geräts als erledigt. Gibt die Zahl zurück.
pub async fn resolve_alerts(conn: &mut PgConnection, device_id: &str) -> Result<u64, RepoError> {
    let result = sqlx::query(
        "update alerts set status = 'resolved', closed_at = now() where device_id = $1 and status = 'open'",
    )
    .bind(device_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected())
}

/// Folgen eines Batteriewechsels: geplante Einsätze des Geräts sind erledigt, offene Meldungen gelöst.
/// Wiederholbar ohne Nebenwirkung.
pub async fn apply_battery_replaced(conn: &mut PgConnection, device_id: &str) -> Result<(), RepoError> {
    sqlx::query("update maintenance_tasks set status = 'completed', completed_at = now() where device_id = $1 and status = 'planned'")
        .bind(device_id)
        .execute(&mut *conn)
        .await?;
    resolve_alerts(conn, device_id).await?;
    Ok(())
}

pub async fn list_alerts(pool: &PgPool, status: Option<AlertStatus>) -> Result<Vec<Alert>, RepoError> {
    Ok(sqlx::query_as(&format!(
        "select {ALERT_COLUMNS} from alerts where ($1::alert_status is null or status = $1) \
         order by (status = 'open') desc, created_at desc, id desc limit 500"
    ))
    .bind(status)
    .fetch_all(pool)
    .await?)
}

/// Weist eine offene Meldung ab.
pub async fn dismiss_alert(pool: &PgPool, id: i64) -> Result<Alert, RepoError> {
    let updated: Option<Alert> = sqlx::query_as(&format!(
        "update alerts set status = 'dismissed', closed_at = now() where id = $1 and status = 'open' returning {ALERT_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match updated {
        Some(alert) => Ok(alert),
        None => {
            let exists: bool = sqlx::query_scalar("select exists(select 1 from alerts where id = $1)")
                .bind(id)
                .fetch_one(pool)
                .await?;
            Err(if exists {
                RepoError::Conflict("Die Meldung ist nicht mehr offen")
            } else {
                RepoError::NotFound("Meldung")
            })
        }
    }
}

pub async fn list_tasks(pool: &PgPool, status: Option<MaintenanceStatus>) -> Result<Vec<MaintenanceTask>, RepoError> {
    Ok(sqlx::query_as(&format!(
        "select {TASK_COLUMNS} from maintenance_tasks where ($1::maintenance_status is null or status = $1) \
         order by (status = 'planned') desc, created_at desc, id desc limit 500"
    ))
    .bind(status)
    .fetch_all(pool)
    .await?)
}

/// Plant einen Batteriewechsel. Pro Gerät ist nur ein geplanter Einsatz erlaubt.
pub async fn plan_task(pool: &PgPool, new: &NewMaintenanceTask) -> Result<MaintenanceTask, RepoError> {
    let result = sqlx::query_as(&format!(
        "insert into maintenance_tasks (device_id, alert_id, note) values ($1, $2, $3) returning {TASK_COLUMNS}"
    ))
    .bind(&new.device_id)
    .bind(new.alert_id)
    .bind(new.note.as_deref().map(str::trim).filter(|n| !n.is_empty()))
    .fetch_one(pool)
    .await;
    match result {
        Ok(task) => Ok(task),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(RepoError::Conflict(
            "Für dieses Gerät ist bereits ein Batteriewechsel geplant",
        )),
        Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => Err(RepoError::NotFound("Gerät oder Meldung")),
        Err(e) => Err(e.into()),
    }
}

/// Bestätigt den Batteriewechsel und meldet `BatteryReplaced`. Alles in einer Transaktion.
pub async fn complete_task(pool: &PgPool, id: i64) -> Result<MaintenanceTask, RepoError> {
    let mut tx = pool.begin().await?;
    let task: Option<MaintenanceTask> = sqlx::query_as(&format!(
        "update maintenance_tasks set status = 'completed', completed_at = now() \
         where id = $1 and status = 'planned' returning {TASK_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(task) = task else {
        let exists: bool = sqlx::query_scalar("select exists(select 1 from maintenance_tasks where id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        return Err(if exists {
            RepoError::Conflict("Der Einsatz ist bereits erledigt")
        } else {
            RepoError::NotFound("Einsatz")
        });
    };
    apply_battery_replaced(&mut tx, &task.device_id).await?;
    notify_event(
        &mut *tx,
        &DomainEvent::BatteryReplaced {
            device_id: task.device_id.clone(),
            at: Utc::now(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(task)
}

/// Gleicht nach einem Neustart ab, was während eines Ausfalls verpasst wurde: Geräte mit
/// Handlungsbedarf ohne offene und ohne abgewiesene Meldung bekommen eine, gesunde Geräte
/// verlieren offene Meldungen. Gibt (eröffnet, gelöst) zurück.
pub async fn reconcile(pool: &PgPool) -> Result<(usize, u64), RepoError> {
    let needing: Vec<(String, shared::DeviceStatus)> = sqlx::query_as(
        "select d.id, d.status from devices d where d.status in ('critical', 'offline') \
         and not exists (select 1 from alerts a where a.device_id = d.id and a.status in ('open', 'dismissed'))",
    )
    .fetch_all(pool)
    .await?;
    let mut opened = 0;
    for (device_id, status) in needing {
        let kind = if status == shared::DeviceStatus::Offline {
            AlertKind::Offline
        } else {
            AlertKind::Critical
        };
        let mut tx = pool.begin().await?;
        if open_alert(&mut tx, &device_id, kind).await?.is_some() {
            opened += 1;
        }
        tx.commit().await?;
    }
    let resolved = sqlx::query(
        "update alerts set status = 'resolved', closed_at = now() where status = 'open' and device_id in \
         (select id from devices where status in ('normal', 'warning'))",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok((opened, resolved))
}
