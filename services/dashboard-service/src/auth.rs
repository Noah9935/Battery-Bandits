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
const MAX_FAILURES: u32 = 5;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// Die angemeldete Person, wie sie die Handler sehen.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
}

impl CurrentUser {
    /// Platzhalter, wenn die Anmeldung abgeschaltet ist (Entwicklung und Tests).
    pub fn guest() -> Self {
        Self {
            id: 0,
            username: "gast".to_owned(),
            display_name: "Gast".to_owned(),
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
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        SESSION_TTL.as_secs(),
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
}

/// Liest `name:passwort:Anzeigename;name2:...`. Fehlt der Anzeigename, gilt der Benutzername. Benutzernamen werden klein
/// geschrieben. Ungültige Einträge (leerer Name oder leeres Passwort) werden mit Grund gemeldet.
pub fn parse_users(spec: &str) -> Result<Vec<NewUser>, String> {
    let mut users = Vec::new();
    for entry in spec.split(';').map(str::trim).filter(|e| !e.is_empty()) {
        let mut parts = entry.splitn(3, ':');
        let username = parts.next().unwrap_or("").trim().to_lowercase();
        let password = parts.next().unwrap_or("").to_owned();
        let display_name = parts
            .next()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| username.clone(), str::to_owned);
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

/// Legt Konten an oder aktualisiert Passwort und Anzeigename.
pub async fn upsert_users(pool: &PgPool, users: &[NewUser]) -> anyhow::Result<()> {
    for user in users {
        let hash = hash_password(&user.password).map_err(|e| anyhow::anyhow!("Passwort-Hash: {e}"))?;
        sqlx::query(
            "insert into users (username, display_name, password_hash) values ($1, $2, $3) \
             on conflict (username) do update set display_name = excluded.display_name, password_hash = excluded.password_hash",
        )
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(hash)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Prüft Benutzername und Passwort. Auch bei unbekanntem Namen wird ein Hash geprüft, damit die Antwortzeit nichts verrät.
pub async fn authenticate(pool: &PgPool, username: &str, password: &str) -> Result<Option<CurrentUser>, sqlx::Error> {
    let row: Option<(i64, String, String)> =
        sqlx::query_as("select id, display_name, password_hash from users where username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await?;
    match row {
        Some((id, display_name, hash)) if verify_password(&hash, password) => Ok(Some(CurrentUser {
            id,
            username: username.to_owned(),
            display_name,
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
pub async fn create_session(pool: &PgPool, user_id: i64) -> Result<String, sqlx::Error> {
    sqlx::query("delete from sessions where expires_at < now()")
        .execute(pool)
        .await?;
    let token = new_token();
    sqlx::query(
        "insert into sessions (token_hash, user_id, expires_at) values ($1, $2, now() + make_interval(secs => $3))",
    )
    .bind(token_hash(&token))
    .bind(user_id)
    .bind(SESSION_TTL.as_secs() as f64)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn user_for_token(pool: &PgPool, token: &str) -> Result<Option<CurrentUser>, sqlx::Error> {
    let row: Option<(i64, String, String)> = sqlx::query_as(
        "select u.id, u.username, u.display_name from sessions s join users u on u.id = s.user_id \
         where s.token_hash = $1 and s.expires_at > now()",
    )
    .bind(token_hash(token))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, username, display_name)| CurrentUser {
        id,
        username,
        display_name,
    }))
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
        let local = session_cookie("t", false);
        assert!(local.contains("HttpOnly") && local.contains("SameSite=Lax") && !local.contains("Secure"));
        assert!(session_cookie("t", true).ends_with("; Secure"));
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
