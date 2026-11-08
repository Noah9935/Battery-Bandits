//! Datenbankoperationen. Ereignisse werden in derselben Transaktion gesendet und
//! erreichen die Empfänger erst nach dem Commit.

use chrono::Utc;
use shared::db::notify_event;
use shared::{
    ActivityEntry, Alert, AlertKind, AlertStatus, DomainEvent, MaintenanceStatus, MaintenanceTask, NewMaintenanceTask,
    TaskComment, TaskKind,
};
use sqlx::{PgConnection, PgPool};

/// Wer eine Änderung auslöst. Kommt vom Dashboard-Service aus der Sitzung, ohne Angabe gilt "System".
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Actor {
    pub username: Option<String>,
    pub name: Option<String>,
}

impl Actor {
    pub fn named(username: &str, name: &str) -> Self {
        Self {
            username: Some(username.to_owned()),
            name: Some(name.to_owned()),
        }
    }

    pub fn display(&self) -> &str {
        self.name.as_deref().unwrap_or("System")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("{0} nicht gefunden")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(&'static str),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Datenbankfehler: {0}")]
    Db(#[from] sqlx::Error),
}

const ALERT_COLUMNS: &str = "id, device_id, kind, status, created_at, closed_at";
const TASK_COLUMNS: &str = "id, device_id, alert_id, status, note, created_at, completed_at, kind, priority, assignee, \
     assignee_name, due_date, created_by_name, completed_by_name, \
     (select count(*) from task_comments c where c.task_id = maintenance_tasks.id) as comment_count";

/// Trägt einen Eintrag ins Aktivitätsprotokoll ein.
pub async fn log_activity(
    conn: &mut PgConnection,
    actor: &Actor,
    device_id: Option<&str>,
    task_id: Option<i64>,
    text: &str,
) -> Result<(), RepoError> {
    sqlx::query("insert into activity_log (actor, actor_name, device_id, task_id, text) values ($1, $2, $3, $4, $5)")
        .bind(actor.username.as_deref())
        .bind(actor.display())
        .bind(device_id)
        .bind(task_id)
        .bind(text)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Eröffnet eine Meldung, falls das Gerät keine offene hat. Gibt die neue Meldung zurück.
///
/// Hat das Gerät schon eine offene Meldung anderer Art (zum Beispiel "offline", das Gerät sendet
/// aber wieder und ist jetzt kritisch), wird deren Art angepasst, damit der Text stimmt.
pub async fn open_alert(conn: &mut PgConnection, device_id: &str, kind: AlertKind) -> Result<Option<Alert>, RepoError> {
    sqlx::query("update alerts set kind = $2 where device_id = $1 and status = 'open' and kind <> $2")
        .bind(device_id)
        .bind(kind)
        .execute(&mut *conn)
        .await?;
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
    sqlx::query("update maintenance_tasks set status = 'completed', completed_at = now() where device_id = $1 and status = 'planned' and kind = 'battery'")
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
pub async fn dismiss_alert(pool: &PgPool, id: i64, actor: &Actor) -> Result<Alert, RepoError> {
    let mut tx = pool.begin().await?;
    let updated: Option<Alert> = sqlx::query_as(&format!(
        "update alerts set status = 'dismissed', closed_at = now() where id = $1 and status = 'open' returning {ALERT_COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    match updated {
        Some(alert) => {
            let text = format!(
                "{} hat die Meldung für {} abgewiesen.",
                actor.display(),
                alert.device_id
            );
            log_activity(&mut tx, actor, Some(&alert.device_id), None, &text).await?;
            tx.commit().await?;
            Ok(alert)
        }
        None => {
            let exists: bool = sqlx::query_scalar("select exists(select 1 from alerts where id = $1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            Err(if exists {
                RepoError::Conflict("Die Meldung ist nicht mehr offen")
            } else {
                RepoError::NotFound("Meldung")
            })
        }
    }
}

/// Offene Einsätze zuerst (dringende vor ruhigen, frühe Fristen vor späten), danach die erledigten.
pub async fn list_tasks(pool: &PgPool, status: Option<MaintenanceStatus>) -> Result<Vec<MaintenanceTask>, RepoError> {
    Ok(sqlx::query_as(&format!(
        "select {TASK_COLUMNS} from maintenance_tasks where ($1::maintenance_status is null or status = $1) \
         order by (status = 'planned') desc, \
                  case when status = 'planned' then priority end desc nulls last, \
                  case when status = 'planned' then due_date end asc nulls last, \
                  created_at desc, id desc limit 500"
    ))
    .bind(status)
    .fetch_all(pool)
    .await?)
}

/// Anzeigename einer zuständigen Person, `Invalid` wenn es das Konto nicht gibt.
async fn assignee_name(conn: &mut PgConnection, username: &str) -> Result<String, RepoError> {
    sqlx::query_scalar("select display_name from users where username = $1")
        .bind(username)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(RepoError::Invalid("Die zuständige Person gibt es nicht"))
}

fn clean(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|t| !t.is_empty())
}

async fn task_changed(conn: &mut PgConnection, task: &MaintenanceTask) -> Result<(), RepoError> {
    notify_event(
        conn,
        &DomainEvent::TaskChanged {
            task_id: task.id,
            device_id: task.device_id.clone(),
        },
    )
    .await?;
    Ok(())
}

/// Plant einen Einsatz. Pro Gerät ist nur ein geplanter Einsatz erlaubt.
pub async fn plan_task(pool: &PgPool, new: &NewMaintenanceTask, actor: &Actor) -> Result<MaintenanceTask, RepoError> {
    let mut tx = pool.begin().await?;
    let assignee = clean(new.assignee.as_deref()).map(str::to_lowercase);
    let assignee_label = match &assignee {
        Some(username) => Some(assignee_name(&mut tx, username).await?),
        None => None,
    };
    let result = sqlx::query_as::<_, MaintenanceTask>(&format!(
        "insert into maintenance_tasks \
            (device_id, alert_id, note, kind, priority, assignee, assignee_name, due_date, created_by, created_by_name) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) returning {TASK_COLUMNS}"
    ))
    .bind(&new.device_id)
    .bind(new.alert_id)
    .bind(clean(new.note.as_deref()))
    .bind(new.kind)
    .bind(new.priority)
    .bind(assignee.as_deref())
    .bind(assignee_label.as_deref())
    .bind(new.due_date)
    .bind(actor.username.as_deref())
    .bind(actor.name.as_deref())
    .fetch_one(&mut *tx)
    .await;
    let task = match result {
        Ok(task) => task,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(RepoError::Conflict("Für dieses Gerät ist bereits ein Einsatz geplant"));
        }
        Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => {
            return Err(RepoError::NotFound("Gerät oder Meldung"));
        }
        Err(e) => return Err(e.into()),
    };
    let who = assignee_label
        .as_deref()
        .map(|n| format!(", zuständig: {n}"))
        .unwrap_or_default();
    let text = format!(
        "{} hat {} für {} geplant{who}.",
        actor.display(),
        task.kind.label(),
        task.device_id
    );
    log_activity(&mut tx, actor, Some(&task.device_id), Some(task.id), &text).await?;
    task_changed(&mut tx, &task).await?;
    tx.commit().await?;
    Ok(task)
}

/// Ändert die Zuständigkeit eines offenen Einsatzes. `None` nimmt sie weg.
pub async fn assign_task(
    pool: &PgPool,
    id: i64,
    assignee: Option<&str>,
    actor: &Actor,
) -> Result<MaintenanceTask, RepoError> {
    let mut tx = pool.begin().await?;
    let username = clean(assignee).map(str::to_lowercase);
    let label = match &username {
        Some(u) => Some(assignee_name(&mut tx, u).await?),
        None => None,
    };
    let task: Option<MaintenanceTask> = sqlx::query_as(&format!(
        "update maintenance_tasks set assignee = $2, assignee_name = $3 \
         where id = $1 and status = 'planned' returning {TASK_COLUMNS}"
    ))
    .bind(id)
    .bind(username.as_deref())
    .bind(label.as_deref())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(task) = task else {
        return Err(missing_or_done(&mut tx, id).await?);
    };
    let text = match (&username, &actor.username) {
        (Some(u), Some(me)) if u == me => format!("{} kümmert sich um {}.", actor.display(), task.device_id),
        (Some(_), _) => format!(
            "{} hat {} für {} zugewiesen.",
            actor.display(),
            label.as_deref().unwrap_or_default(),
            task.device_id
        ),
        (None, _) => format!(
            "{} hat die Zuständigkeit für {} entfernt.",
            actor.display(),
            task.device_id
        ),
    };
    log_activity(&mut tx, actor, Some(&task.device_id), Some(task.id), &text).await?;
    task_changed(&mut tx, &task).await?;
    tx.commit().await?;
    Ok(task)
}

async fn missing_or_done(conn: &mut PgConnection, id: i64) -> Result<RepoError, RepoError> {
    let exists: bool = sqlx::query_scalar("select exists(select 1 from maintenance_tasks where id = $1)")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(if exists {
        RepoError::Conflict("Der Einsatz ist bereits erledigt")
    } else {
        RepoError::NotFound("Einsatz")
    })
}

/// Bestätigt den Einsatz. Bei einem Batteriewechsel wird zusätzlich `BatteryReplaced` gemeldet,
/// sodass das Gerät wieder als geladen gilt. Alles in einer Transaktion.
pub async fn complete_task(pool: &PgPool, id: i64, actor: &Actor) -> Result<MaintenanceTask, RepoError> {
    let mut tx = pool.begin().await?;
    let task: Option<MaintenanceTask> = sqlx::query_as(&format!(
        "update maintenance_tasks set status = 'completed', completed_at = now(), completed_by_name = $2 \
         where id = $1 and status = 'planned' returning {TASK_COLUMNS}"
    ))
    .bind(id)
    .bind(actor.name.as_deref())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(task) = task else {
        return Err(missing_or_done(&mut tx, id).await?);
    };
    if task.kind == TaskKind::Battery {
        apply_battery_replaced(&mut tx, &task.device_id).await?;
        notify_event(
            &mut *tx,
            &DomainEvent::BatteryReplaced {
                device_id: task.device_id.clone(),
                at: Utc::now(),
            },
        )
        .await?;
    }
    let text = format!(
        "{} hat {} bei {} erledigt.",
        actor.display(),
        task.kind.label(),
        task.device_id
    );
    log_activity(&mut tx, actor, Some(&task.device_id), Some(task.id), &text).await?;
    task_changed(&mut tx, &task).await?;
    tx.commit().await?;
    Ok(task)
}

pub async fn list_comments(pool: &PgPool, task_id: i64) -> Result<Vec<TaskComment>, RepoError> {
    let exists: bool = sqlx::query_scalar("select exists(select 1 from maintenance_tasks where id = $1)")
        .bind(task_id)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Err(RepoError::NotFound("Einsatz"));
    }
    Ok(sqlx::query_as(
        "select id, task_id, author_name, body, created_at from task_comments where task_id = $1 order by created_at, id",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?)
}

pub async fn add_comment(pool: &PgPool, task_id: i64, body: &str, actor: &Actor) -> Result<TaskComment, RepoError> {
    let body = body.trim();
    if body.is_empty() {
        return Err(RepoError::Invalid("Der Kommentar ist leer"));
    }
    if body.chars().count() > 2000 {
        return Err(RepoError::Invalid("Der Kommentar ist zu lang (höchstens 2000 Zeichen)"));
    }
    let mut tx = pool.begin().await?;
    let device_id: Option<String> = sqlx::query_scalar("select device_id from maintenance_tasks where id = $1")
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(device_id) = device_id else {
        return Err(RepoError::NotFound("Einsatz"));
    };
    let comment: TaskComment = sqlx::query_as(
        "insert into task_comments (task_id, author, author_name, body) values ($1, $2, $3, $4) \
         returning id, task_id, author_name, body, created_at",
    )
    .bind(task_id)
    .bind(actor.username.as_deref())
    .bind(actor.display())
    .bind(body)
    .fetch_one(&mut *tx)
    .await?;
    let text = format!("{} hat bei {} kommentiert.", actor.display(), device_id);
    log_activity(&mut tx, actor, Some(&device_id), Some(task_id), &text).await?;
    notify_event(&mut *tx, &DomainEvent::TaskChanged { task_id, device_id }).await?;
    tx.commit().await?;
    Ok(comment)
}

/// Neueste zuerst, optional nur für ein Gerät.
pub async fn list_activity(
    pool: &PgPool,
    device_id: Option<&str>,
    limit: i64,
) -> Result<Vec<ActivityEntry>, RepoError> {
    Ok(sqlx::query_as(
        "select id, at, actor_name, device_id, text from activity_log \
         where ($1::text is null or device_id = $1) order by at desc, id desc limit $2",
    )
    .bind(device_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?)
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
