//! Datenbankoperationen. Ereignisse werden in derselben Transaktion gesendet und
//! erreichen die Empfänger erst nach dem Commit.

use chrono::{DateTime, Utc};
use shared::db::notify_event;
use shared::notify::NotificationRule;
use shared::{
    ActivityEntry, Alert, AlertKind, AlertStatus, BatterySwap, DomainEvent, InboxItem, MaintenanceStatus,
    MaintenanceTask, NewMaintenanceTask, NewTour, TaskComment, TaskKind, Tour, TourUpdate,
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
    #[error("{0}")]
    Forbidden(&'static str),
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

/// Legt einen Eintrag im Postfach einer Person an.
pub async fn notify_user(
    conn: &mut PgConnection,
    username: &str,
    kind: &str,
    text: &str,
    device_id: Option<&str>,
    task_id: Option<i64>,
) -> Result<(), RepoError> {
    sqlx::query("insert into inbox_items (username, kind, text, device_id, task_id) values ($1, $2, $3, $4, $5)")
        .bind(username)
        .bind(kind)
        .bind(text)
        .bind(device_id)
        .bind(task_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Neue Meldungen gehen an alle Konten.
/// Wer eine Meldung zu diesem Gerät bekommt: die Personen, die sich beim Node eingetragen haben. Hat der Node
/// niemanden, bekommen alle Konten die Meldung.
pub const RECIPIENTS_SQL: &str = "select u.username from users u \
     where u.username in (select username from node_subscriptions where device_id = $1) \
        or not exists (select 1 from node_subscriptions s join users su on su.username = s.username where s.device_id = $1)";

/// Empfänger einer Meldung zu diesem Gerät (siehe [`RECIPIENTS_SQL`]).
pub async fn alert_recipients(pool: &PgPool, device_id: &str) -> Result<Vec<String>, RepoError> {
    Ok(sqlx::query_scalar(RECIPIENTS_SQL)
        .bind(device_id)
        .fetch_all(pool)
        .await?)
}

/// Geräte, bei denen sich die Person eingetragen hat.
pub async fn subscriptions(pool: &PgPool, username: Option<&str>) -> Result<Vec<String>, RepoError> {
    let Some(username) = username else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar("select device_id from node_subscriptions where username = $1 order by device_id")
            .bind(username)
            .fetch_all(pool)
            .await?,
    )
}

/// Trägt die Person bei einem Node ein. Wiederholbar ohne Nebenwirkung.
pub async fn subscribe(pool: &PgPool, username: Option<&str>, device_id: &str) -> Result<(), RepoError> {
    let username = username.ok_or(RepoError::Invalid("Eintragen geht nur mit Anmeldung"))?;
    match sqlx::query("insert into node_subscriptions (username, device_id) values ($1, $2) on conflict do nothing")
        .bind(username)
        .bind(device_id)
        .execute(pool)
        .await
    {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(e)) if e.is_foreign_key_violation() => Err(RepoError::NotFound("Gerät")),
        Err(e) => Err(e.into()),
    }
}

/// Trägt die Person wieder aus. Wiederholbar ohne Nebenwirkung.
pub async fn unsubscribe(pool: &PgPool, username: Option<&str>, device_id: &str) -> Result<(), RepoError> {
    let username = username.ok_or(RepoError::Invalid("Austragen geht nur mit Anmeldung"))?;
    sqlx::query("delete from node_subscriptions where username = $1 and device_id = $2")
        .bind(username)
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Merkliste und Touren gibt es nur mit Anmeldung, sie gehören einer Person.
fn require_user<'a>(username: Option<&'a str>, what: &'static str) -> Result<&'a str, RepoError> {
    username.ok_or(RepoError::Invalid(what))
}

/// Die Merkliste der Person: die Nodes, bei denen sie die Batterie wechseln will.
pub async fn pins(pool: &PgPool, username: Option<&str>) -> Result<Vec<String>, RepoError> {
    let Some(username) = username else {
        return Ok(Vec::new());
    };
    Ok(
        sqlx::query_scalar("select device_id from user_pins where username = $1 order by created_at, device_id")
            .bind(username)
            .fetch_all(pool)
            .await?,
    )
}

/// Setzt Nodes auf die Merkliste. Unbekannte Kennungen werden übergangen, Wiederholen schadet nicht.
pub async fn add_pins(pool: &PgPool, username: Option<&str>, ids: &[String]) -> Result<(), RepoError> {
    let username = require_user(username, "Merken geht nur mit Anmeldung")?;
    let ids = shared::tours::clean_ids(ids);
    sqlx::query(
        "insert into user_pins (username, device_id) select $1, id from devices where id = any($2) on conflict do nothing",
    )
    .bind(username)
    .bind(&ids)
    .execute(pool)
    .await?;
    Ok(())
}

/// Nimmt einen Node von der Merkliste. Wiederholbar ohne Nebenwirkung.
pub async fn remove_pin(pool: &PgPool, username: Option<&str>, device_id: &str) -> Result<(), RepoError> {
    let username = require_user(username, "Merken geht nur mit Anmeldung")?;
    sqlx::query("delete from user_pins where username = $1 and device_id = $2")
        .bind(username)
        .bind(device_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Leert die Merkliste der Person.
pub async fn clear_pins(pool: &PgPool, username: Option<&str>) -> Result<(), RepoError> {
    let username = require_user(username, "Merken geht nur mit Anmeldung")?;
    sqlx::query("delete from user_pins where username = $1")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(())
}

const TOUR_SELECT: &str = "select t.id, t.name, t.owner, coalesce(u.display_name, t.owner) as owner_name, t.device_ids, \
     coalesce((select array_agg(s.username order by s.username) from tour_shares s where s.tour_id = t.id and t.owner = $1), '{}') as shared_with, \
     (t.owner = $1) as mine, t.updated_at \
     from tours t left join users u on u.username = t.owner";

/// Eigene Touren und die, die andere mit der Person geteilt haben: erst die eigenen, nach Namen sortiert.
pub async fn list_tours(pool: &PgPool, username: Option<&str>) -> Result<Vec<Tour>, RepoError> {
    let Some(username) = username else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_as(&format!(
        "{TOUR_SELECT} where (t.owner = $1 or exists (select 1 from tour_shares s where s.tour_id = t.id and s.username = $1)) \
         and t.deleted_at is null \
         order by (t.owner <> $1), lower(t.name), t.id"
    ))
    .bind(username)
    .fetch_all(pool)
    .await?)
}

async fn tour_for(conn: &mut PgConnection, username: &str, id: i64) -> Result<Tour, RepoError> {
    sqlx::query_as(&format!(
        "{TOUR_SELECT} where t.id = $2 and t.deleted_at is null \
         and (t.owner = $1 or exists (select 1 from tour_shares s where s.tour_id = t.id and s.username = $1))"
    ))
    .bind(username)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(RepoError::NotFound("Tour"))
}

/// Eine Tour, bei der die Person Besitzerin ist. Wer sie nur sieht, darf sie nicht teilen oder löschen.
async fn own_tour(conn: &mut PgConnection, username: &str, id: i64) -> Result<Tour, RepoError> {
    let tour = tour_for(conn, username, id).await?;
    if !tour.mine {
        return Err(RepoError::Forbidden(
            "Nur wer die Tour angelegt hat, kann sie teilen oder löschen",
        ));
    }
    Ok(tour)
}

fn name_taken(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            RepoError::Conflict("Eine Tour mit diesem Namen gibt es schon")
        }
        e => e.into(),
    }
}

/// Legt eine Tour an, auf Wunsch gleich geteilt.
pub async fn create_tour(pool: &PgPool, actor: &Actor, new: &NewTour) -> Result<Tour, RepoError> {
    let username = require_user(actor.username.as_deref(), "Touren gibt es nur mit Anmeldung")?;
    let (name, devices) = new.cleaned().map_err(RepoError::Invalid)?;
    let shares = shared::tours::clean_shares(&new.share_with, username).map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar("insert into tours (name, owner, device_ids) values ($1, $2, $3) returning id")
        .bind(&name)
        .bind(username)
        .bind(&devices)
        .fetch_one(&mut *tx)
        .await
        .map_err(name_taken)?;
    replace_shares(&mut tx, actor, &name, id, &shares).await?;
    let tour = tour_for(&mut tx, username, id).await?;
    tx.commit().await?;
    Ok(tour)
}

/// Benennt eine Tour um und ändert ihre Nodes. Das dürfen alle, die die Tour sehen: die Besitzerin oder der Besitzer
/// und die, mit denen sie geteilt ist. Teilen und Löschen bleibt bei der Besitzerin oder dem Besitzer.
pub async fn update_tour(pool: &PgPool, actor: &Actor, id: i64, update: &TourUpdate) -> Result<Tour, RepoError> {
    let username = require_user(actor.username.as_deref(), "Touren gibt es nur mit Anmeldung")?;
    let name = update
        .name
        .as_deref()
        .map(shared::tours::clean_tour_name)
        .transpose()
        .map_err(RepoError::Invalid)?;
    let devices = update
        .device_ids
        .as_deref()
        .map(shared::tours::clean_tour_devices)
        .transpose()
        .map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    tour_for(&mut tx, username, id).await?;
    sqlx::query(
        "update tours set name = coalesce($2, name), device_ids = coalesce($3, device_ids), updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(devices)
    .execute(&mut *tx)
    .await
    .map_err(name_taken)?;
    let tour = tour_for(&mut tx, username, id).await?;
    tx.commit().await?;
    Ok(tour)
}

/// Löscht eine eigene Tour samt Freigaben.
pub async fn delete_tour(pool: &PgPool, username: Option<&str>, id: i64) -> Result<(), RepoError> {
    let username = require_user(username, "Touren gibt es nur mit Anmeldung")?;
    let mut tx = pool.begin().await?;
    own_tour(&mut tx, username, id).await?;
    // Die Tour kommt in den Papierkorb und bleibt dort 30 Tage.
    sqlx::query("update tours set deleted_at = now() where id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    purge_expired_tours(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Löscht endgültig, was länger als 30 Tage im Papierkorb liegt.
async fn purge_expired_tours(conn: &mut PgConnection) -> Result<(), RepoError> {
    sqlx::query("delete from tours where deleted_at < now() - make_interval(days => $1::int)")
        .bind(shared::TRASH_DAYS as i32)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Die gelöschten Touren der Person, die noch im Papierkorb liegen: die zuletzt gelöschten zuerst.
pub async fn trashed_tours(pool: &PgPool, username: Option<&str>) -> Result<Vec<shared::TrashedTour>, RepoError> {
    let Some(username) = username else {
        return Ok(Vec::new());
    };
    let mut conn = pool.acquire().await?;
    purge_expired_tours(&mut conn).await?;
    Ok(sqlx::query_as(
        "select id, name, cardinality(device_ids) as node_count, deleted_at from tours \
         where owner = $1 and deleted_at is not null order by deleted_at desc, id desc",
    )
    .bind(username)
    .fetch_all(&mut *conn)
    .await?)
}

/// Holt eine Tour aus dem Papierkorb zurück. Gibt es inzwischen eine Tour mit dem Namen, ist das ein Fehler.
pub async fn restore_tour(pool: &PgPool, username: Option<&str>, id: i64) -> Result<(), RepoError> {
    let username = require_user(username, "Touren gibt es nur mit Anmeldung")?;
    let restored = sqlx::query(
        "update tours set deleted_at = null, updated_at = now() where id = $1 and owner = $2 and deleted_at is not null",
    )
    .bind(id)
    .bind(username)
    .execute(pool)
    .await
    .map_err(name_taken)?
    .rows_affected();
    if restored == 0 {
        return Err(RepoError::NotFound("Tour im Papierkorb"));
    }
    Ok(())
}

/// Löscht eine Tour aus dem Papierkorb endgültig.
pub async fn purge_tour(pool: &PgPool, username: Option<&str>, id: i64) -> Result<(), RepoError> {
    let username = require_user(username, "Touren gibt es nur mit Anmeldung")?;
    let removed = sqlx::query("delete from tours where id = $1 and owner = $2 and deleted_at is not null")
        .bind(id)
        .bind(username)
        .execute(pool)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(RepoError::NotFound("Tour im Papierkorb"));
    }
    Ok(())
}

/// Ersetzt die Freigaben. Neue Empfänger bekommen einen Eintrag im Postfach. Wen es als Konto nicht gibt, ist ein Fehler.
async fn replace_shares(
    conn: &mut PgConnection,
    actor: &Actor,
    tour_name: &str,
    id: i64,
    usernames: &[String],
) -> Result<(), RepoError> {
    let known: Vec<String> = sqlx::query_scalar("select username from users where username = any($1)")
        .bind(usernames)
        .fetch_all(&mut *conn)
        .await?;
    if known.len() != usernames.len() {
        return Err(RepoError::Invalid("Eine der Personen gibt es nicht"));
    }
    let before: Vec<String> = sqlx::query_scalar("select username from tour_shares where tour_id = $1")
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    sqlx::query("delete from tour_shares where tour_id = $1 and not (username = any($2))")
        .bind(id)
        .bind(usernames)
        .execute(&mut *conn)
        .await?;
    for username in usernames.iter().filter(|u| !before.contains(u)) {
        sqlx::query("insert into tour_shares (tour_id, username) values ($1, $2) on conflict do nothing")
            .bind(id)
            .bind(username)
            .execute(&mut *conn)
            .await?;
        let text = format!(
            "{} hat die Tour \u{201e}{tour_name}\u{201c} mit dir geteilt.",
            actor.display()
        );
        notify_user(conn, username, "shared", &text, None, None).await?;
    }
    Ok(())
}

/// Teilt eine eigene Tour mit genau diesen Personen (ersetzt die bisherigen Freigaben).
pub async fn share_tour(pool: &PgPool, actor: &Actor, id: i64, usernames: &[String]) -> Result<Tour, RepoError> {
    let username = require_user(actor.username.as_deref(), "Touren gibt es nur mit Anmeldung")?;
    let shares = shared::tours::clean_shares(usernames, username).map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let tour = own_tour(&mut tx, username, id).await?;
    replace_shares(&mut tx, actor, &tour.name, id, &shares).await?;
    let tour = tour_for(&mut tx, username, id).await?;
    tx.commit().await?;
    Ok(tour)
}

/// Eine mit einem geteilte Tour wieder aus der eigenen Liste nehmen. Die Tour der anderen Person bleibt bestehen.
pub async fn leave_tour(pool: &PgPool, username: Option<&str>, id: i64) -> Result<(), RepoError> {
    let username = require_user(username, "Touren gibt es nur mit Anmeldung")?;
    let removed = sqlx::query("delete from tour_shares where tour_id = $1 and username = $2")
        .bind(id)
        .bind(username)
        .execute(pool)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(RepoError::NotFound("Geteilte Tour"));
    }
    Ok(())
}

async fn inbox_for_alert(conn: &mut PgConnection, alert: &Alert) -> Result<(), RepoError> {
    sqlx::query(
        "insert into inbox_items (username, kind, text, device_id) \
         select username, 'alert', $2, $1 from (select u.username from users u \
            where u.username in (select username from node_subscriptions where device_id = $1) \
               or not exists (select 1 from node_subscriptions s join users su on su.username = s.username where s.device_id = $1)) r",
    )
    .bind(&alert.device_id)
    .bind(format!("{}: {}", alert.device_id, alert.kind.label()))
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
        inbox_for_alert(&mut *conn, alert).await?;
        let event = DomainEvent::AlertCreated {
            alert_id: alert.id,
            device_id: device_id.to_owned(),
            kind,
        };
        notify_event(&mut *conn, &event).await?;
    }
    Ok(alert)
}

/// Eröffnet eine Meldung wegen einer Auffälligkeit. Gibt es schon eine offene Meldung für das Gerät oder
/// wurde gleicher Art in den letzten 24 Stunden eine geschlossen, passiert nichts: Wer eine Meldung abgewiesen
/// hat, soll nicht sofort wieder damit belästigt werden.
pub async fn open_anomaly(
    conn: &mut PgConnection,
    device_id: &str,
    kind: AlertKind,
) -> Result<Option<Alert>, RepoError> {
    let alert: Option<Alert> = sqlx::query_as(&format!(
        "insert into alerts (device_id, kind) \
         select $1, $2 where not exists ( \
             select 1 from alerts where device_id = $1 and kind = $2 \
               and coalesce(closed_at, now()) > now() - interval '24 hours') \
         on conflict (device_id) where status = 'open' do nothing returning {ALERT_COLUMNS}"
    ))
    .bind(device_id)
    .bind(kind)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(alert) = &alert {
        inbox_for_alert(&mut *conn, alert).await?;
        let event = DomainEvent::AlertCreated {
            alert_id: alert.id,
            device_id: device_id.to_owned(),
            kind,
        };
        notify_event(&mut *conn, &event).await?;
    }
    Ok(alert)
}

/// Das Gerät ist wieder in Ordnung: Meldungen, die am Status hängen, sind erledigt. Auffälligkeiten bleiben.
pub async fn resolve_recovered(conn: &mut PgConnection, device_id: &str) -> Result<u64, RepoError> {
    let result = sqlx::query(
        "update alerts set status = 'resolved', closed_at = now() \
         where device_id = $1 and status = 'open' and kind in ('critical', 'offline', 'fault')",
    )
    .bind(device_id)
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected())
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
    if let Some(user) = assignee.as_deref().filter(|u| Some(*u) != actor.username.as_deref()) {
        let hint = format!(
            "{} hat dir einen Einsatz an {} zugewiesen.",
            actor.display(),
            task.device_id
        );
        notify_user(&mut tx, user, "assigned", &hint, Some(&task.device_id), Some(task.id)).await?;
    }
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
    if let Some(user) = username.as_deref().filter(|u| Some(*u) != actor.username.as_deref()) {
        let hint = format!(
            "{} hat dir den Einsatz an {} zugewiesen.",
            actor.display(),
            task.device_id
        );
        notify_user(&mut tx, user, "assigned", &hint, Some(&task.device_id), Some(task.id)).await?;
    }
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
        "update maintenance_tasks set status = 'completed', completed_at = now(), completed_by_name = $2, \
         assignee_name = case when assignee is null and $3::text is not null then $2 else assignee_name end, \
         assignee = coalesce(assignee, $3) \
         where id = $1 and status = 'planned' returning {TASK_COLUMNS}"
    ))
    .bind(id)
    .bind(actor.name.as_deref())
    // War niemand zuständig, ist es automatisch die Person, die den Einsatz abhakt.
    .bind(actor.username.as_deref())
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
                soc_percent: None,
                price_cents: None,
                model: None,
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

/// Der beim Batteriewechsel abgeschlossene Zyklus der alten Batterie.
#[derive(sqlx::FromRow)]
struct CycleRow {
    id: i64,
    started_at: DateTime<Utc>,
    discharged_ah: f64,
    price_cents: Option<i32>,
    model: Option<String>,
}

/// Macht das Erledigen eines Einsatzes rückgängig (Falscheintragung). Ein Einsatz, der erst beim Erledigen entstand
/// (Schnellerfassung ohne Plan), verschwindet wieder, ein vorher geplanter wird wieder offen. Bei einem
/// Batteriewechsel geht auch das Gerät auf die vorherige Batterie zurück (Einbaudatum, entnommene Ladung, Preis,
/// Modell). Das geht nur für den letzten Wechsel des Geräts.
pub async fn undo_task(pool: &PgPool, id: i64, actor: &Actor) -> Result<MaintenanceTask, RepoError> {
    let mut tx = pool.begin().await?;
    let task: Option<MaintenanceTask> = sqlx::query_as(&format!(
        "select {TASK_COLUMNS} from maintenance_tasks where id = $1 for update"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(task) = task else {
        return Err(RepoError::NotFound("Einsatz"));
    };
    let Some(completed_at) = task
        .completed_at
        .filter(|_| task.status == MaintenanceStatus::Completed)
    else {
        return Err(RepoError::Conflict("Der Einsatz ist gar nicht erledigt"));
    };
    if task.kind == TaskKind::Battery {
        let later: bool = sqlx::query_scalar(
            "select exists(select 1 from maintenance_tasks where device_id = $1 and kind = 'battery' \
             and status = 'completed' and completed_at > $2 and id <> $3)",
        )
        .bind(&task.device_id)
        .bind(completed_at)
        .bind(task.id)
        .fetch_one(&mut *tx)
        .await?;
        if later {
            return Err(RepoError::Conflict(
                "Danach gab es schon einen weiteren Wechsel, nur der letzte lässt sich zurücknehmen",
            ));
        }
        // Der Zyklus der alten Batterie, der beim Wechsel abgeschlossen wurde.
        let cycle: Option<CycleRow> = sqlx::query_as(
            "select id, started_at, discharged_ah, price_cents, model from battery_cycles \
             where device_id = $1 and ended_at >= $2 - interval '1 minute' order by ended_at asc limit 1",
        )
        .bind(&task.device_id)
        .bind(completed_at)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(CycleRow {
            id: cycle_id,
            started_at,
            discharged_ah,
            price_cents,
            model,
        }) = cycle
        {
            sqlx::query(
                "update devices set battery_installed_at = $2, discharged_ah = $3, battery_price_cents = $4, \
                 battery_model = $5, discharge_rate_pct_per_hour = null, forecast_hours_remaining = null \
                 where id = $1",
            )
            .bind(&task.device_id)
            .bind(started_at)
            .bind(discharged_ah)
            .bind(price_cents)
            .bind(model)
            .execute(&mut *tx)
            .await?;
            sqlx::query("delete from battery_cycles where id = $1")
                .bind(cycle_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    // Entstand der Einsatz erst beim Erledigen (gleiche Zeit), gab es ihn vorher nicht: Er wird gelöscht.
    let created_with_swap = (completed_at - task.created_at).num_seconds().abs() < 2;
    let result = if created_with_swap {
        sqlx::query("delete from maintenance_tasks where id = $1")
            .bind(task.id)
            .execute(&mut *tx)
            .await?;
        task.clone()
    } else {
        sqlx::query_as(&format!(
            "update maintenance_tasks set status = 'planned', completed_at = null, completed_by_name = null \
             where id = $1 returning {TASK_COLUMNS}"
        ))
        .bind(task.id)
        .fetch_one(&mut *tx)
        .await?
    };
    let text = format!(
        "{} hat das Erledigen von {} bei {} rückgängig gemacht.",
        actor.display(),
        task.kind.label(),
        task.device_id
    );
    log_activity(&mut tx, actor, Some(&task.device_id), None, &text).await?;
    task_changed(&mut tx, &task).await?;
    tx.commit().await?;
    Ok(result)
}

/// Preis der eingebauten Batterie eines Geräts, auch nachträglich. `None` entfernt ihn.
pub async fn set_device_battery_price(
    pool: &PgPool,
    device_id: &str,
    request: &shared::SetPrice,
    actor: &Actor,
) -> Result<(), RepoError> {
    request.validate().map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let found: Option<String> =
        sqlx::query_scalar("update devices set battery_price_cents = $2 where id = $1 returning id")
            .bind(device_id)
            .bind(request.price_cents)
            .fetch_optional(&mut *tx)
            .await?;
    if found.is_none() {
        return Err(RepoError::NotFound("Gerät"));
    }
    let text = price_log_text(actor, "der laufenden Batterie", device_id, request.price_cents);
    log_activity(&mut tx, actor, Some(device_id), None, &text).await?;
    tx.commit().await?;
    Ok(())
}

/// Preis einer abgeschlossenen Batterie (Zyklus), auch nachträglich. `None` entfernt ihn.
pub async fn set_cycle_price(
    pool: &PgPool,
    cycle_id: i64,
    request: &shared::SetPrice,
    actor: &Actor,
) -> Result<(), RepoError> {
    request.validate().map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let device_id: Option<String> =
        sqlx::query_scalar("update battery_cycles set price_cents = $2 where id = $1 returning device_id")
            .bind(cycle_id)
            .bind(request.price_cents)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(device_id) = device_id else {
        return Err(RepoError::NotFound("Batteriezyklus"));
    };
    let text = price_log_text(actor, "einer früheren Batterie", &device_id, request.price_cents);
    log_activity(&mut tx, actor, Some(&device_id), None, &text).await?;
    tx.commit().await?;
    Ok(())
}

fn price_log_text(actor: &Actor, which: &str, device_id: &str, price_cents: Option<i32>) -> String {
    match price_cents {
        Some(cents) => format!(
            "{} hat den Preis {which} von {device_id} nachgetragen: {}.",
            actor.display(),
            shared::euro_text(cents)
        ),
        None => format!("{} hat den Preis {which} von {device_id} entfernt.", actor.display()),
    }
}

/// Trägt ein Gerät aus: Es verschwindet aus Übersicht, Karte und Meldungen. Offene Meldungen werden gelöst, geplante
/// Einsätze entfallen. Messwerte und erledigte Einsätze bleiben.
pub async fn retire_device(
    pool: &PgPool,
    device_id: &str,
    request: &shared::RetireDevice,
    actor: &Actor,
) -> Result<(), RepoError> {
    request.validate().map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let note = clean(request.note.as_deref());
    let done: Option<String> = sqlx::query_scalar(
        "update devices set retired_at = now(), retired_reason = $2, retired_note = $3 \
         where id = $1 and retired_at is null returning id",
    )
    .bind(device_id)
    .bind(request.reason.as_str())
    .bind(note)
    .fetch_optional(&mut *tx)
    .await?;
    if done.is_none() {
        let exists: bool = sqlx::query_scalar("select exists(select 1 from devices where id = $1)")
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
        return Err(if exists {
            RepoError::Conflict("Das Gerät ist schon ausgemustert")
        } else {
            RepoError::NotFound("Gerät")
        });
    }
    resolve_alerts(&mut tx, device_id).await?;
    sqlx::query("delete from maintenance_tasks where device_id = $1 and status = 'planned'")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    let text = match note {
        Some(note) => format!(
            "{} hat {device_id} ausgemustert ({}). Notiz: {note}",
            actor.display(),
            request.reason.label()
        ),
        None => format!(
            "{} hat {device_id} ausgemustert ({}).",
            actor.display(),
            request.reason.label()
        ),
    };
    log_activity(&mut tx, actor, Some(device_id), None, &text).await?;
    tx.commit().await?;
    Ok(())
}

/// Nimmt ein ausgemustertes Gerät wieder auf. Die Überwachung bewertet es beim nächsten Durchlauf neu.
pub async fn reactivate_device(pool: &PgPool, device_id: &str, actor: &Actor) -> Result<(), RepoError> {
    let mut tx = pool.begin().await?;
    let done: Option<String> = sqlx::query_scalar(
        "update devices set retired_at = null, retired_reason = null, retired_note = null \
         where id = $1 and retired_at is not null returning id",
    )
    .bind(device_id)
    .fetch_optional(&mut *tx)
    .await?;
    if done.is_none() {
        let exists: bool = sqlx::query_scalar("select exists(select 1 from devices where id = $1)")
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
        return Err(if exists {
            RepoError::Conflict("Das Gerät ist gar nicht ausgemustert")
        } else {
            RepoError::NotFound("Gerät")
        });
    }
    let text = format!("{} hat {device_id} wieder aufgenommen.", actor.display());
    log_activity(&mut tx, actor, Some(device_id), None, &text).await?;
    tx.commit().await?;
    Ok(())
}

/// Schnellerfassung: Batteriewechsel mit einem Tipp bestätigen, auch ohne vorher geplanten Einsatz.
///
/// Ein geplanter Batteriewechsel des Geräts wird abgeschlossen, sonst entsteht ein bereits erledigter Einsatz,
/// damit der Wechsel im Protokoll und in der Liste der Einsätze steht. Offene Meldungen werden gelöst,
/// `BatteryReplaced` setzt den Ladestand zurück. Alles in einer Transaktion.
pub async fn swap_battery(pool: &PgPool, swap: &BatterySwap, actor: &Actor) -> Result<MaintenanceTask, RepoError> {
    swap.validate().map_err(RepoError::Invalid)?;
    let device_id = swap.device_id.trim();
    let mut tx = pool.begin().await?;
    // Aus dem Katalog gelten Name und Preis des Eintrags, sonst die freien Angaben.
    let (model, price_cents) = match swap.battery_type_id {
        Some(type_id) => {
            let row: Option<(String, Option<i32>)> =
                sqlx::query_as("select name, price_cents from battery_types where id = $1 and archived_at is null")
                    .bind(type_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let (name, price) = row.ok_or(RepoError::NotFound("Batterie"))?;
            (Some(name), price)
        }
        None => (swap.model.clone(), swap.price_cents),
    };
    let known: bool = sqlx::query_scalar("select exists(select 1 from devices where id = $1)")
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await?;
    if !known {
        return Err(RepoError::NotFound("Gerät"));
    }
    // Wer gewechselt hat: die ausgewählte Person, sonst wer angemeldet ist.
    let technician = clean(swap.technician.as_deref()).map(str::to_lowercase);
    let done_by = match &technician {
        Some(username) => Some(assignee_name(&mut tx, username).await?),
        None => actor.name.clone(),
    };
    let note = clean(swap.note.as_deref());
    // Zuständig ist, wer gewechselt hat: die gewählte Person, sonst wer angemeldet ist. Ein schon zugewiesener
    // Einsatz behält seine Zuständigkeit.
    let doer: Option<String> = technician.clone().or_else(|| actor.username.clone());
    let doer_name: Option<String> = doer.as_ref().and(done_by.clone());
    let planned: Option<MaintenanceTask> = sqlx::query_as(&format!(
        "update maintenance_tasks set status = 'completed', completed_at = now(), completed_by_name = $2, \
         note = case when $3::text is null then note when note is null then $3 else note || chr(10) || $3 end, \
         assignee_name = case when assignee is null and $4::text is not null then $5 else assignee_name end, \
         assignee = coalesce(assignee, $4) \
         where device_id = $1 and status = 'planned' and kind = 'battery' returning {TASK_COLUMNS}"
    ))
    .bind(device_id)
    .bind(done_by.as_deref())
    .bind(note)
    .bind(doer.as_deref())
    .bind(doer_name.as_deref())
    .fetch_optional(&mut *tx)
    .await?;
    let task = match planned {
        Some(task) => task,
        None => {
            sqlx::query_as(&format!(
                "insert into maintenance_tasks \
                 (device_id, status, note, kind, created_by, created_by_name, completed_at, completed_by_name, \
                  assignee, assignee_name) \
                 values ($1, 'completed', $2, 'battery', $3, $4, now(), $5, $6, $7) returning {TASK_COLUMNS}"
            ))
            .bind(device_id)
            .bind(note)
            .bind(actor.username.as_deref())
            .bind(actor.name.as_deref())
            .bind(done_by.as_deref())
            .bind(doer.as_deref())
            .bind(doer_name.as_deref())
            .fetch_one(&mut *tx)
            .await?
        }
    };
    apply_battery_replaced(&mut tx, device_id).await?;
    notify_event(
        &mut *tx,
        &DomainEvent::BatteryReplaced {
            device_id: device_id.to_owned(),
            at: Utc::now(),
            soc_percent: swap.soc_percent,
            price_cents,
            model: model.clone(),
        },
    )
    .await?;
    let who = done_by.as_deref().unwrap_or_else(|| actor.display());
    let price = price_cents
        .map(|cents| format!(" Preis: {}.", shared::euro_text(cents)))
        .unwrap_or_default();
    let model = clean(model.as_deref())
        .map(|m| format!(" Modell: {m}."))
        .unwrap_or_default();
    let text = match note {
        Some(note) => format!("{who} hat die Batterie von {device_id} gewechselt.{model}{price} Notiz: {note}"),
        None => format!("{who} hat die Batterie von {device_id} gewechselt.{model}{price}"),
    };
    log_activity(&mut tx, actor, Some(device_id), Some(task.id), &text).await?;
    task_changed(&mut tx, &task).await?;
    tx.commit().await?;
    Ok(task)
}

const RULE_COLUMNS: &str = "enabled, only_critical, whatsapp, whatsapp_number, whatsapp_key, telegram, telegram_chat_id, \
 sms, sms_number, email, email_address";

/// Weiterleitungsregel einer Person. Ohne gespeicherte Regel gilt die Vorgabe (alles aus).
pub async fn notification_rule(pool: &PgPool, username: &str) -> Result<NotificationRule, RepoError> {
    let rule: Option<NotificationRule> = sqlx::query_as(&format!(
        "select {RULE_COLUMNS} from notification_rules where username = $1"
    ))
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(rule.unwrap_or(NotificationRule {
        only_critical: true,
        ..NotificationRule::default()
    }))
}

/// Speichert die Regel einer Person. Ziele werden ohne Leerzeichen am Rand abgelegt.
pub async fn save_notification_rule(
    pool: &PgPool,
    username: &str,
    rule: &NotificationRule,
) -> Result<NotificationRule, RepoError> {
    Ok(sqlx::query_as(&format!(
        "insert into notification_rules (username, {RULE_COLUMNS}) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         on conflict (username) do update set enabled = excluded.enabled, only_critical = excluded.only_critical, \
         whatsapp = excluded.whatsapp, whatsapp_number = excluded.whatsapp_number, \
         whatsapp_key = excluded.whatsapp_key, \
         telegram = excluded.telegram, telegram_chat_id = excluded.telegram_chat_id, \
         sms = excluded.sms, sms_number = excluded.sms_number, \
         email = excluded.email, email_address = excluded.email_address, updated_at = now() \
         returning {RULE_COLUMNS}"
    ))
    .bind(username)
    .bind(rule.enabled)
    .bind(rule.only_critical)
    .bind(rule.whatsapp)
    .bind(rule.whatsapp_number.trim())
    .bind(rule.whatsapp_key())
    .bind(rule.telegram)
    .bind(rule.telegram_chat_id.trim())
    .bind(rule.sms)
    .bind(rule.sms_number.trim())
    .bind(rule.email)
    .bind(rule.email_address.trim())
    .fetch_one(pool)
    .await?)
}

/// Alle eingeschalteten Regeln mit dem Namen der Person.
pub async fn active_notification_rules(pool: &PgPool) -> Result<Vec<(String, NotificationRule)>, RepoError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        username: String,
        #[sqlx(flatten)]
        rule: NotificationRule,
    }
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "select username, {RULE_COLUMNS} from notification_rules where enabled order by username"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.username, r.rule)).collect())
}

/// Hält fest, ob eine Weiterleitung geklappt hat. Der Text enthält nie Zugangsdaten.
pub async fn log_notification(
    pool: &PgPool,
    username: &str,
    channel: shared::notify::Channel,
    alert_id: Option<i64>,
    ok: bool,
    detail: &str,
) -> Result<(), RepoError> {
    let detail: String = detail.chars().take(300).collect();
    sqlx::query("insert into notification_log (username, channel, alert_id, ok, detail) values ($1, $2, $3, $4, $5)")
        .bind(username)
        .bind(channel.as_str())
        .bind(alert_id)
        .bind(ok)
        .bind(detail)
        .execute(pool)
        .await?;
    Ok(())
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
    let row: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("select device_id, assignee, created_by from maintenance_tasks where id = $1")
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((device_id, assignee, created_by)) = row else {
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
    // Wer zuständig ist oder den Einsatz angelegt hat, soll Kommentare anderer nicht übersehen.
    let mut hear: Vec<&str> = [assignee.as_deref(), created_by.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    hear.sort_unstable();
    hear.dedup();
    let preview: String = body.chars().take(80).collect();
    let hint = format!("{} hat bei {} kommentiert: {}", actor.display(), device_id, preview);
    for user in hear.into_iter().filter(|u| Some(*u) != actor.username.as_deref()) {
        notify_user(&mut tx, user, "comment", &hint, Some(&device_id), Some(task_id)).await?;
    }
    notify_event(&mut *tx, &DomainEvent::TaskChanged { task_id, device_id }).await?;
    tx.commit().await?;
    Ok(comment)
}

/// Das Postfach einer Person, neueste zuerst. Gelesenes bleibt an seinem Platz, nur die Markierung fällt weg.
/// Ohne Namen ist es leer.
pub async fn list_inbox(pool: &PgPool, username: Option<&str>, limit: i64) -> Result<Vec<InboxItem>, RepoError> {
    let Some(username) = username else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_as(
        "select id, at, kind, text, device_id, task_id, read_at is not null as read from inbox_items \
         where username = $1 order by at desc, id desc limit $2",
    )
    .bind(username)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?)
}

/// Markiert Einträge als gelesen (`None`: alle). Gibt die Zahl der geänderten Einträge zurück.
pub async fn mark_read(pool: &PgPool, username: Option<&str>, ids: Option<&[i64]>) -> Result<u64, RepoError> {
    let Some(username) = username else {
        return Ok(0);
    };
    let result = sqlx::query(
        "update inbox_items set read_at = now() \
         where username = $1 and read_at is null and ($2::bigint[] is null or id = any($2))",
    )
    .bind(username)
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Neueste zuerst, optional nur für ein Gerät.
pub async fn list_activity(
    pool: &PgPool,
    device_id: Option<&str>,
    limit: i64,
) -> Result<Vec<ActivityEntry>, RepoError> {
    Ok(sqlx::query_as(
        "select id, at, actor_name, device_id, text, task_id from activity_log \
         where ($1::text is null or device_id = $1) order by at desc, id desc limit $2",
    )
    .bind(device_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?)
}

/// Der Batteriekatalog, alphabetisch. Archivierte Einträge fehlen.
pub async fn list_battery_types(pool: &PgPool) -> Result<Vec<shared::BatteryType>, RepoError> {
    Ok(sqlx::query_as(
        "select id, name, price_cents, note from battery_types where archived_at is null order by lower(name), id",
    )
    .fetch_all(pool)
    .await?)
}

fn catalogue_conflict(e: sqlx::Error) -> RepoError {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => RepoError::Conflict("Diese Batterie gibt es schon"),
        other => RepoError::Db(other),
    }
}

/// Legt eine Batterie im Katalog an.
pub async fn create_battery_type(
    pool: &PgPool,
    new: &shared::NewBatteryType,
    actor: &Actor,
) -> Result<shared::BatteryType, RepoError> {
    new.validate().map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let created: shared::BatteryType = sqlx::query_as(
        "insert into battery_types (name, price_cents, note, created_by_name) values ($1, $2, $3, $4) \
         returning id, name, price_cents, note",
    )
    .bind(new.name.trim())
    .bind(new.price_cents)
    .bind(clean(new.note.as_deref()))
    .bind(actor.name.as_deref())
    .fetch_one(&mut *tx)
    .await
    .map_err(catalogue_conflict)?;
    let text = format!(
        "{} hat die Batterie {} in den Katalog aufgenommen.",
        actor.display(),
        created.menu_text()
    );
    log_activity(&mut tx, actor, None, None, &text).await?;
    tx.commit().await?;
    Ok(created)
}

/// Ändert Name, Preis oder Notiz einer Batterie im Katalog. Frühere Wechsel behalten ihre Angaben.
pub async fn update_battery_type(
    pool: &PgPool,
    id: i64,
    new: &shared::NewBatteryType,
    actor: &Actor,
) -> Result<shared::BatteryType, RepoError> {
    new.validate().map_err(RepoError::Invalid)?;
    let mut tx = pool.begin().await?;
    let updated: Option<shared::BatteryType> = sqlx::query_as(
        "update battery_types set name = $2, price_cents = $3, note = $4 where id = $1 and archived_at is null \
         returning id, name, price_cents, note",
    )
    .bind(id)
    .bind(new.name.trim())
    .bind(new.price_cents)
    .bind(clean(new.note.as_deref()))
    .fetch_optional(&mut *tx)
    .await
    .map_err(catalogue_conflict)?;
    let updated = updated.ok_or(RepoError::NotFound("Batterie"))?;
    let text = format!(
        "{} hat die Batterie {} im Katalog geändert.",
        actor.display(),
        updated.menu_text()
    );
    log_activity(&mut tx, actor, None, None, &text).await?;
    tx.commit().await?;
    Ok(updated)
}

/// Nimmt eine Batterie aus dem Katalog. Sie bleibt archiviert, frühere Wechsel behalten ihre Angaben.
pub async fn archive_battery_type(pool: &PgPool, id: i64, actor: &Actor) -> Result<(), RepoError> {
    let mut tx = pool.begin().await?;
    let name: Option<String> = sqlx::query_scalar(
        "update battery_types set archived_at = now() where id = $1 and archived_at is null returning name",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let name = name.ok_or(RepoError::NotFound("Batterie"))?;
    let text = format!("{} hat die Batterie {name} aus dem Katalog genommen.", actor.display());
    log_activity(&mut tx, actor, None, None, &text).await?;
    tx.commit().await?;
    Ok(())
}

/// Löscht endgültig, was länger als 30 Tage aus dem Katalog genommen ist.
async fn purge_expired_battery_types(conn: &mut PgConnection) -> Result<(), RepoError> {
    sqlx::query("delete from battery_types where archived_at < now() - make_interval(days => $1::int)")
        .bind(shared::TRASH_DAYS as i32)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Die aus dem Katalog genommenen Batterien im Papierkorb, die zuletzt entfernten zuerst.
pub async fn trashed_battery_types(pool: &PgPool) -> Result<Vec<shared::TrashedBattery>, RepoError> {
    let mut conn = pool.acquire().await?;
    purge_expired_battery_types(&mut conn).await?;
    Ok(sqlx::query_as(
        "select id, name, price_cents, archived_at from battery_types \
         where archived_at is not null order by archived_at desc, id desc",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Holt eine Batterie aus dem Papierkorb zurück in den Katalog. Gibt es inzwischen eine mit dem Namen, ist das ein Fehler.
pub async fn restore_battery_type(pool: &PgPool, id: i64, actor: &Actor) -> Result<(), RepoError> {
    let mut tx = pool.begin().await?;
    let name: Option<String> = sqlx::query_scalar(
        "update battery_types set archived_at = null where id = $1 and archived_at is not null returning name",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(catalogue_conflict)?;
    let name = name.ok_or(RepoError::NotFound("Batterie im Papierkorb"))?;
    let text = format!(
        "{} hat die Batterie {name} zurück in den Katalog geholt.",
        actor.display()
    );
    log_activity(&mut tx, actor, None, None, &text).await?;
    tx.commit().await?;
    Ok(())
}

/// Löscht eine Batterie aus dem Papierkorb endgültig.
pub async fn purge_battery_type(pool: &PgPool, id: i64) -> Result<(), RepoError> {
    let removed = sqlx::query("delete from battery_types where id = $1 and archived_at is not null")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if removed == 0 {
        return Err(RepoError::NotFound("Batterie im Papierkorb"));
    }
    Ok(())
}

/// Die Batterien, die den erfundenen Geräten eingebaut werden. Sie stehen mit Händlerpreisen im Katalog (Migration 0020).
/// Alle sind 12-V-AGM-Batterien der Klasse 80 bis 100 Ah, so wie die Simulation sie rechnet (80 Ah Nennkapazität).
pub const SIMULATION_BATTERIES: [&str; 5] = [
    "Effekta BTL 12-80 (AGM, 12 V, 80 Ah)",
    "Effekta BTL 12-100 (AGM, 12 V, 100 Ah)",
    "Victron AGM Deep Cycle 12 V 90 Ah",
    "Dometic NDS GreenPower GP80 (AGM, 12 V, 80 Ah)",
    "Electronicx AGM Marine 80 Ah (12 V)",
];

/// Die Nummer hinter SIM, zum Beispiel 7 bei SIM07. Andere Kennungen haben keine.
fn simulation_number(id: &str) -> Option<usize> {
    id.strip_prefix("SIM")?.parse::<usize>().ok().filter(|n| *n >= 1)
}

/// Trägt bei erfundenen Geräten (Herkunft "simulation") Batteriemodell und Preis aus dem Katalog ein, solange beides
/// fehlt. Die Zuordnung ist fest: SIM01 bekommt das erste Modell der Liste, SIM02 das zweite und so fort, SIM06 wieder
/// das erste. So bleibt die Simulation von Lauf zu Lauf gleich und jedes Modell hat gleich viele Geräte. Echte Geräte
/// bleiben unberührt, ebenso Geräte, bei denen schon ein Modell oder Preis steht (etwa nach einem Batteriewechsel).
/// Ist ein Modell aus dem Katalog genommen, bleibt sein Gerät ohne Angabe, bis es wieder da ist.
/// Gibt die Zahl der geänderten Geräte zurück.
pub async fn assign_simulated_batteries(pool: &PgPool) -> Result<usize, RepoError> {
    let names: Vec<String> = SIMULATION_BATTERIES.iter().map(|name| (*name).to_owned()).collect();
    let catalogue: Vec<(String, Option<i32>)> =
        sqlx::query_as("select name, price_cents from battery_types where archived_at is null and name = any($1)")
            .bind(&names)
            .fetch_all(pool)
            .await?;
    // Ein Platz je Modell, in der Reihenfolge der Liste. Fehlt ein Modell im Katalog, ist sein Platz leer.
    let slots: Vec<Option<&(String, Option<i32>)>> = names
        .iter()
        .map(|name| catalogue.iter().find(|(known, _)| known == name))
        .collect();
    if slots.iter().all(Option::is_none) {
        return Ok(0);
    }
    let devices: Vec<String> = sqlx::query_scalar(
        "select id from devices where data_source = 'simulation' and battery_model is null \
         and battery_price_cents is null order by id",
    )
    .fetch_all(pool)
    .await?;
    let mut changed = 0;
    for id in devices {
        let Some(number) = simulation_number(&id) else {
            continue;
        };
        let Some(Some((name, price))) = slots.get((number - 1) % slots.len()) else {
            continue;
        };
        let done = sqlx::query(
            "update devices set battery_model = $2, battery_price_cents = $3 \
             where id = $1 and battery_model is null and battery_price_cents is null",
        )
        .bind(&id)
        .bind(name.as_str())
        .bind(*price)
        .execute(pool)
        .await?
        .rows_affected();
        changed += usize::try_from(done).unwrap_or(0);
    }
    Ok(changed)
}

/// Gleicht nach einem Neustart ab, was während eines Ausfalls verpasst wurde: Geräte mit
/// Handlungsbedarf ohne offene und ohne abgewiesene Meldung bekommen eine, gesunde Geräte
/// verlieren offene Meldungen. Gibt (eröffnet, gelöst) zurück.
pub async fn reconcile(pool: &PgPool) -> Result<(usize, u64), RepoError> {
    let settings = shared::db::load_anomaly_settings(pool).await?;
    let needing: Vec<(String, shared::DeviceStatus, Option<f32>)> = sqlx::query_as(
        "select d.id, d.status, d.soc_percent from devices d where d.status in ('critical', 'offline') \
         and d.retired_at is null and not exists (select 1 from alerts a where a.device_id = d.id and a.status in ('open', 'dismissed'))",
    )
    .fetch_all(pool)
    .await?;
    let mut opened = 0;
    for (device_id, status, soc) in needing {
        let kind = if status == shared::DeviceStatus::Offline {
            crate::rules::offline_kind(soc, &settings)
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
        "update alerts set status = 'resolved', closed_at = now() \
         where status = 'open' and kind in ('critical', 'offline', 'fault') and device_id in \
         (select id from devices where status in ('normal', 'warning'))",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok((opened, resolved))
}
