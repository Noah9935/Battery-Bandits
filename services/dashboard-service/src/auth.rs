//! Anmeldung: Konten, Sitzungen per Cookie, Passwörter mit Argon2.
//!
//! * Das Sitzungs-Token ist 256 Bit zufällig, in der Datenbank steht nur sein SHA-256-Hash.
//! * Das Cookie ist `HttpOnly` und `SameSite=Lax`, hinter HTTPS zusätzlich `Secure`.
//! * Konten kommen aus der Umgebungsvariable `BB_USERS` (Format `name:passwort:Anzeigename;...`) und werden beim
//!   Start angelegt oder aktualisiert. Es gibt keine Selbstregistrierung.
//! * Zu viele falsche Anmeldungen je Benutzername werden kurz gesperrt.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub const COOKIE_NAME: &str = "bb_session";
/// Gültigkeit einer Sitzung.
pub const SESSION_TTL: Duration = Duration::from_secs(14 * 24 * 3600);
/// Gültigkeit mit "Angemeldet bleiben": gut ein Jahr. Das Token liegt als HttpOnly-Cookie im Browser.
pub const REMEMBER_TTL: Duration = Duration::from_secs(400 * 24 * 3600);
const MAX_FAILURES: u32 = 5;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// Die angemeldete Person, wie sie die Handler sehen.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub avatar: u8,
    pub admin: bool,
}

impl CurrentUser {
    /// Platzhalter, wenn die Anmeldung abgeschaltet ist (Entwicklung und Tests).
    pub fn guest() -> Self {
        Self {
            id: 0,
            username: "gast".to_owned(),
            display_name: "Gast".to_owned(),
            avatar: 0,
            admin: false,
        }
    }
}

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt)?.to_string())
}

pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Neues zufälliges Token (64 Hexzeichen).
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash, unter dem ein Token in der Datenbank steht.
pub fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Wert eines Cookies aus dem `Cookie`-Kopf.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

/// `Set-Cookie` für eine neue Sitzung. `Secure` nur hinter HTTPS.
pub fn session_cookie(token: &str, secure: bool, ttl: Duration) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        ttl.as_secs(),
        if secure { "; Secure" } else { "" }
    )
}

/// `Set-Cookie`, das die Sitzung beim Browser löscht.
pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

pub use shared::api::{decode_header, encode_header};

#[derive(Debug, Clone, PartialEq)]
pub struct NewUser {
    pub username: String,
    pub password: String,
    pub display_name: String,
    /// Mit dem vierten Feld `admin` in der Konfiguration.
    pub admin: bool,
}

/// Liest `name:passwort:Anzeigename;name2:...`. Fehlt der Anzeigename, gilt der Benutzername. Benutzernamen werden klein
/// geschrieben. Ungültige Einträge (leerer Name oder leeres Passwort) werden mit Grund gemeldet.
pub fn parse_users(spec: &str) -> Result<Vec<NewUser>, String> {
    let mut users = Vec::new();
    for entry in spec.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let mut parts = entry.splitn(4, ':');
        let username = parts.next().unwrap_or("").trim().to_lowercase();
        let password = parts.next().unwrap_or("").to_owned();
        let display_name = parts
            .next()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| username.clone(), str::to_owned);
        let admin = match parts.next().map(|f| f.trim().to_lowercase()).as_deref() {
            None | Some("") | Some("user") => false,
            Some("admin") => true,
            Some(other) => {
                return Err(format!(
                    "Rolle \"{other}\" von \"{username}\" ist unbekannt (erlaubt: admin oder user)"
                ));
            }
        };
        if !(2..=32).contains(&username.chars().count()) {
            return Err(format!("Benutzername \"{username}\" muss 2 bis 32 Zeichen haben"));
        }
        if password.len() < 4 {
            return Err(format!(
                "Passwort von \"{username}\" ist zu kurz (mindestens 4 Zeichen)"
            ));
        }
        users.push(NewUser {
            username,
            password,
            display_name,
            admin,
        });
    }
    Ok(users)
}

/// Zählt falsche Anmeldungen je Benutzername in einem Zeitfenster.
#[derive(Default)]
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginLimiter {
    /// Darf `username` es gerade versuchen?
    pub fn allowed(&self, username: &str, now: Instant) -> bool {
        let failures = self.failures.lock().expect("Sperre nicht vergiftet");
        match failures.get(username) {
            Some((count, since)) if now.duration_since(*since) < FAILURE_WINDOW => *count < MAX_FAILURES,
            _ => true,
        }
    }

    pub fn record_failure(&self, username: &str, now: Instant) {
        let mut failures = self.failures.lock().expect("Sperre nicht vergiftet");
        let entry = failures.entry(username.to_owned()).or_insert((0, now));
        if now.duration_since(entry.1) >= FAILURE_WINDOW {
            *entry = (0, now);
        }
        entry.0 += 1;
    }

    pub fn clear(&self, username: &str) {
        self.failures.lock().expect("Sperre nicht vergiftet").remove(username);
    }
}

// ---------- Datenbank ----------

/// Legt Konten an oder aktualisiert den Anzeigenamen. Das Passwort aus der Konfiguration gilt, solange die Person
/// ihres nicht selbst geändert hat. Danach gehört es ihr und bleibt über Neustarts erhalten.
pub async fn upsert_users(pool: &PgPool, users: &[NewUser]) -> anyhow::Result<()> {
    for user in users {
        let hash = hash_password(&user.password).map_err(|e| anyhow::anyhow!("Passwort-Hash: {e}"))?;
        sqlx::query(
            "insert into users (username, display_name, password_hash, role) values ($1, $2, $3, $4) \
             on conflict (username) do update set display_name = excluded.display_name, \
                password_hash = case when users.password_changed_at is null then excluded.password_hash else users.password_hash end, \
                role = case when excluded.role = 'admin' then 'admin' else users.role end",
        )
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(hash)
        .bind(if user.admin { "admin" } else { "user" })
        .execute(pool)
        .await?;
    }
    // Es gibt immer mindestens einen Admin: Fehlt einer, wird das erste Konto der Konfiguration einer.
    if let Some(first) = users.first() {
        sqlx::query(
            "update users set role = 'admin' where username = $1 and not exists (select 1 from users where role = 'admin')",
        )
        .bind(&first.username)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Prüft Benutzername und Passwort. Auch bei unbekanntem Namen wird ein Hash geprüft, damit die Antwortzeit nichts verrät.
pub async fn authenticate(pool: &PgPool, username: &str, password: &str) -> Result<Option<CurrentUser>, sqlx::Error> {
    let row: Option<(i64, String, String, i16, String)> =
        sqlx::query_as("select id, display_name, password_hash, avatar, role from users where username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await?;
    match row {
        Some((id, display_name, hash, avatar, role)) if verify_password(&hash, password) => Ok(Some(CurrentUser {
            id,
            username: username.to_owned(),
            display_name,
            avatar: shared::api::avatar_from_db(avatar),
            admin: role == "admin",
        })),
        Some(_) => Ok(None),
        None => {
            let _ = verify_password(
                "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                password,
            );
            Ok(None)
        }
    }
}

/// Neue Sitzung, gibt das Token zurück (nur hier im Klartext). Alte, abgelaufene Sitzungen werden dabei aufgeräumt.
pub async fn create_session(pool: &PgPool, user_id: i64, ttl: Duration) -> Result<String, sqlx::Error> {
    sqlx::query("delete from sessions where expires_at < now()")
        .execute(pool)
        .await?;
    let token = new_token();
    sqlx::query(
        "insert into sessions (token_hash, user_id, expires_at) values ($1, $2, now() + make_interval(secs => $3))",
    )
    .bind(token_hash(&token))
    .bind(user_id)
    .bind(ttl.as_secs() as f64)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn user_for_token(pool: &PgPool, token: &str) -> Result<Option<CurrentUser>, sqlx::Error> {
    let row: Option<(i64, String, String, i16, String)> = sqlx::query_as(
        "select u.id, u.username, u.display_name, u.avatar, u.role from sessions s join users u on u.id = s.user_id \
         where s.token_hash = $1 and s.expires_at > now()",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, username, display_name, avatar, role)| CurrentUser {
        id,
        username,
        display_name,
        avatar: shared::api::avatar_from_db(avatar),
        admin: role == "admin",
    }))
}

/// Speichert das gewählte Profilbild der Person.
pub async fn set_avatar(pool: &PgPool, user_id: i64, avatar: u8) -> sqlx::Result<()> {
    sqlx::query("update users set avatar = $2 where id = $1")
        .bind(user_id)
        .bind(i16::from(avatar))
        .execute(pool)
        .await?;
    Ok(())
}

/// Setzt ein neues Passwort. Alle anderen Sitzungen der Person enden (nur `keep_token` bleibt), damit ein
/// vergessenes Gerät mit dem alten Passwort nicht weiter angemeldet ist.
pub async fn change_password(
    pool: &PgPool,
    user_id: i64,
    new_password: &str,
    keep_token: Option<&str>,
) -> anyhow::Result<()> {
    let hash = hash_password(new_password).map_err(|e| anyhow::anyhow!("Passwort-Hash: {e}"))?;
    let mut tx = pool.begin().await?;
    sqlx::query("update users set password_hash = $2, password_changed_at = now() where id = $1")
        .bind(user_id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from sessions where user_id = $1 and token_hash <> $2")
        .bind(user_id)
        .bind(keep_token.map(token_hash).unwrap_or_default())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Fehler der Kontenverwaltung.
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("Das Konto gibt es nicht.")]
    NotFound,
    #[error("Der Benutzername ist schon vergeben.")]
    Taken,
    #[error("Es muss mindestens ein Admin bleiben.")]
    LastAdmin,
    #[error("Das eigene Konto lässt sich nicht löschen.")]
    OwnAccount,
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Datenbankfehler: {0}")]
    Db(#[from] sqlx::Error),
}

/// Alle Konten, nach Anzeigename.
pub async fn list_accounts(pool: &PgPool) -> Result<Vec<shared::api::Account>, sqlx::Error> {
    let rows: Vec<(String, String, i16, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "select username, display_name, avatar, role, created_at from users order by lower(display_name), username",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(username, display_name, avatar, role, created_at)| shared::api::Account {
                username,
                display_name,
                avatar: shared::api::avatar_from_db(avatar),
                admin: role == "admin",
                created_at,
            },
        )
        .collect())
}

/// Legt ein Konto an. Das Profilbild wird zufällig zugewiesen.
pub async fn create_account(pool: &PgPool, new: &shared::api::NewAccount) -> Result<(), AdminError> {
    let username = new.username.trim().to_lowercase();
    shared::api::check_username(&username).map_err(AdminError::Invalid)?;
    shared::api::check_display_name(&new.display_name).map_err(AdminError::Invalid)?;
    shared::api::check_new_password("", &new.password).map_err(AdminError::Invalid)?;
    let hash =
        hash_password(&new.password).map_err(|_| AdminError::Invalid("Das Passwort ließ sich nicht verarbeiten."))?;
    let result = sqlx::query(
        "insert into users (username, display_name, password_hash, role, password_changed_at) values ($1, $2, $3, $4, now())",
    )
    .bind(&username)
    .bind(new.display_name.trim())
    .bind(hash)
    .bind(if new.admin { "admin" } else { "user" })
    .execute(pool)
    .await;
    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(AdminError::Taken),
        Err(e) => Err(e.into()),
    }
}

async fn admin_count(conn: &mut sqlx::PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("select count(*) from users where role = 'admin'")
        .fetch_one(&mut *conn)
        .await
}

/// Ändert Anzeigename, Rolle oder Passwort eines Kontos. Der letzte Admin lässt sich nicht herabstufen. Ein neues
/// Passwort beendet alle Sitzungen der Person.
pub async fn update_account(
    pool: &PgPool,
    username: &str,
    update: &shared::api::AccountUpdate,
) -> Result<(), AdminError> {
    let mut tx = pool.begin().await?;
    let current: Option<(i64, String)> = sqlx::query_as("select id, role from users where username = $1 for update")
        .bind(username)
        .fetch_optional(&mut *tx)
        .await?;
    let Some((id, role)) = current else {
        return Err(AdminError::NotFound);
    };
    if let Some(name) = &update.display_name {
        shared::api::check_display_name(name).map_err(AdminError::Invalid)?;
        sqlx::query("update users set display_name = $2 where id = $1")
            .bind(id)
            .bind(name.trim())
            .execute(&mut *tx)
            .await?;
    }
    if let Some(admin) = update.admin {
        if !admin && role == "admin" && admin_count(&mut tx).await? <= 1 {
            return Err(AdminError::LastAdmin);
        }
        sqlx::query("update users set role = $2 where id = $1")
            .bind(id)
            .bind(if admin { "admin" } else { "user" })
            .execute(&mut *tx)
            .await?;
    }
    if let Some(password) = &update.password {
        shared::api::check_new_password("", password).map_err(AdminError::Invalid)?;
        let hash =
            hash_password(password).map_err(|_| AdminError::Invalid("Das Passwort ließ sich nicht verarbeiten."))?;
        sqlx::query("update users set password_hash = $2, password_changed_at = now() where id = $1")
            .bind(id)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
        sqlx::query("delete from sessions where user_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Löscht ein Konto samt Sitzungen, Postfach, Eintragungen bei Nodes und Weiterleitungsregel. Nicht das eigene und
/// nicht den letzten Admin.
pub async fn delete_account(pool: &PgPool, username: &str, acting_username: &str) -> Result<(), AdminError> {
    if username == acting_username {
        return Err(AdminError::OwnAccount);
    }
    let mut tx = pool.begin().await?;
    let current: Option<String> = sqlx::query_scalar("select role from users where username = $1 for update")
        .bind(username)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(role) = current else {
        return Err(AdminError::NotFound);
    };
    if role == "admin" && admin_count(&mut tx).await? <= 1 {
        return Err(AdminError::LastAdmin);
    }
    for table in [
        "inbox_items",
        "node_subscriptions",
        "notification_rules",
        "notification_log",
    ] {
        sqlx::query(&format!("delete from {table} where username = $1"))
            .bind(username)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("delete from users where username = $1")
        .bind(username)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_session(pool: &PgPool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("delete from sessions where token_hash = $1")
        .bind(token_hash(token))
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip_and_wrong_password() {
        let hash = hash_password("geheim-123").unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password(&hash, "geheim-123"));
        assert!(!verify_password(&hash, "falsch"));
        assert!(!verify_password("kein-hash", "geheim-123"));
        assert_ne!(hash, hash_password("geheim-123").unwrap(), "jedes Mal ein anderes Salz");
    }

    #[test]
    fn tokens_are_long_unique_and_hashed_consistently() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert_eq!(token_hash(&a), token_hash(&a));
        assert_ne!(token_hash(&a), a);
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_the_session_cookie_among_others() {
        let header = "theme=dark; bb_session=abc123; other=1";
        assert_eq!(cookie_value(header, COOKIE_NAME), Some("abc123"));
        assert_eq!(cookie_value(header, "theme"), Some("dark"));
        assert_eq!(cookie_value(header, "fehlt"), None);
        assert_eq!(cookie_value("", COOKIE_NAME), None);
    }

    #[test]
    fn cookies_are_http_only_and_secure_behind_https() {
        let local = session_cookie("t", false, SESSION_TTL);
        assert!(local.contains("HttpOnly") && local.contains("SameSite=Lax") && !local.contains("Secure"));
        assert!(session_cookie("t", true, SESSION_TTL).ends_with("; Secure"));
        assert!(clear_cookie(false).contains("Max-Age=0"));
    }

    #[test]
    fn header_encoding_roundtrips_umlauts() {
        for text in ["Noah Boufercha", "Jörg Müller", "a%b"] {
            let encoded = encode_header(text);
            assert!(encoded.is_ascii());
            assert_eq!(decode_header(&encoded), text);
        }
    }

    #[test]
    fn parses_user_lists() {
        let users = parse_users("Noah:geheim1:Noah Boufercha; fynn:geheim2 ;gast:gastpw:").unwrap();
        assert_eq!(users.len(), 3);
        assert_eq!(
            (users[0].username.as_str(), users[0].display_name.as_str()),
            ("noah", "Noah Boufercha")
        );
        assert_eq!(users[1].display_name, "fynn");
        assert_eq!(
            users[2].display_name, "gast",
            "leerer Anzeigename fällt auf den Namen zurück"
        );
        assert!(parse_users("").unwrap().is_empty());
    }

    #[test]
    fn the_fourth_field_makes_an_admin() {
        let users = parse_users("noah:geheim1:Noah Boufercha:admin; fynn:geheim2:Fynn:user; lena:geheim3").unwrap();
        assert_eq!(users.iter().map(|u| u.admin).collect::<Vec<_>>(), [true, false, false]);
        assert_eq!(users[0].display_name, "Noah Boufercha");
        assert!(parse_users("noah:geheim1:Noah:chef").unwrap_err().contains("unbekannt"));
    }

    #[test]
    fn rejects_bad_user_entries() {
        assert!(parse_users("x:geheim1").unwrap_err().contains("2 bis 32"));
        assert!(parse_users("noah:abc").unwrap_err().contains("zu kurz"));
        assert!(parse_users("noah").unwrap_err().contains("zu kurz"));
    }

    #[test]
    fn limiter_blocks_after_five_failures_and_recovers() {
        let limiter = LoginLimiter::default();
        let t0 = Instant::now();
        for _ in 0..5 {
            assert!(limiter.allowed("noah", t0));
            limiter.record_failure("noah", t0);
        }
        assert!(!limiter.allowed("noah", t0), "gesperrt");
        assert!(limiter.allowed("fynn", t0), "andere Namen sind nicht betroffen");
        assert!(
            limiter.allowed("noah", t0 + FAILURE_WINDOW + Duration::from_secs(1)),
            "nach dem Fenster wieder frei"
        );
        limiter.clear("noah");
        assert!(limiter.allowed("noah", t0));
    }
}
