//! Typen der REST-Schnittstelle zwischen Dashboard-Service und Frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::forecast::Forecast;

/// Zeitraum der Verlaufsansicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Range {
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
}

impl Range {
    pub const ALL: [Range; 3] = [Range::Day, Range::Week, Range::Month];

    /// Wert für die Adresse (`?range=24h`).
    pub fn as_str(self) -> &'static str {
        match self {
            Range::Day => "24h",
            Range::Week => "7d",
            Range::Month => "30d",
        }
    }

    /// Anzeigename für die Oberfläche.
    pub fn label(self) -> &'static str {
        match self {
            Range::Day => "24 Std.",
            Range::Week => "7 Tage",
            Range::Month => "30 Tage",
        }
    }

    /// Länge des Zeitraums als Postgres-Intervall.
    pub fn window_sql(self) -> &'static str {
        match self {
            Range::Day => "24 hours",
            Range::Week => "7 days",
            Range::Month => "30 days",
        }
    }

    /// Breite eines Mittelungsfensters, damit jede Kurve höchstens rund 300 Punkte hat.
    pub fn bucket_sql(self) -> &'static str {
        match self {
            Range::Day => "5 minutes",
            Range::Week => "30 minutes",
            Range::Month => "2 hours",
        }
    }
}

impl std::str::FromStr for Range {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Range::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unbekannter Zeitraum: {s}"))
    }
}

/// Ein (gemittelter) Punkt der Verlaufskurve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct HistoryPoint {
    pub time: DateTime<Utc>,
    pub soc_percent: f32,
    pub voltage_mv: f32,
    pub current_ma: f32,
    pub temperature_c: Option<f32>,
}

/// Verlauf eines Geräts mit Prognose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub device_id: String,
    pub range: Range,
    pub points: Vec<HistoryPoint>,
    /// Restlaufzeit-Prognose aus den letzten 24 Stunden, falls das Gerät entladen wird.
    pub forecast: Option<Forecast>,
}

/// Verlauf des Ladezustands eines Geräts für die kleine Trendlinie in der Tabelle:
/// Stundenmittel der letzten 24 Stunden bis zur letzten Messung des Geräts, älteste zuerst.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trend {
    pub device_id: String,
    pub points: Vec<f32>,
}

/// Kopfwerte dürfen nur sichtbares ASCII enthalten: alles andere wird prozentkodiert.
pub fn encode_header(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Gegenstück zu [`encode_header`].
pub fn decode_header(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| {
                std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
            })
            .flatten();
        match decoded {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Angemeldete Person (Antwort von `/api/auth/me` und `/api/auth/login`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub username: String,
    pub display_name: String,
    /// Nummer des Profilbilds, bei der Anlage des Kontos zufällig zugewiesen und in den Optionen wählbar.
    #[serde(default)]
    pub avatar: u8,
    /// Darf Konten verwalten.
    #[serde(default)]
    pub admin: bool,
}

/// Ein Konto in der Verwaltung der Admins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub display_name: String,
    pub avatar: u8,
    pub admin: bool,
    pub created_at: DateTime<Utc>,
}

/// Neues Konto, vom Admin angelegt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewAccount {
    pub username: String,
    pub display_name: String,
    pub password: String,
    #[serde(default)]
    pub admin: bool,
}

/// Änderung an einem Konto. Was fehlt, bleibt, wie es ist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccountUpdate {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub admin: Option<bool>,
    /// Neues Passwort (Zurücksetzen durch den Admin).
    #[serde(default)]
    pub password: Option<String>,
}

/// Prüft einen Benutzernamen: 2 bis 32 Zeichen, nur Kleinbuchstaben, Ziffern, Punkt, Binde- und Unterstrich.
pub fn check_username(username: &str) -> Result<(), &'static str> {
    let length = username.chars().count();
    if !(2..=32).contains(&length) {
        return Err("Der Benutzername braucht 2 bis 32 Zeichen.");
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'))
    {
        return Err(
            "Der Benutzername darf nur Kleinbuchstaben, Ziffern, Punkt, Bindestrich und Unterstrich enthalten.",
        );
    }
    Ok(())
}

/// Prüft den Anzeigenamen: 1 bis 80 Zeichen.
pub fn check_display_name(name: &str) -> Result<(), &'static str> {
    let length = name.trim().chars().count();
    if (1..=80).contains(&length) {
        Ok(())
    } else {
        Err("Der Anzeigename braucht 1 bis 80 Zeichen.")
    }
}

/// Eine verworfene Nachricht aus dem Protokoll: Was kam an, und warum wurde sie nicht gespeichert?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvalidMessage {
    pub id: i64,
    pub received_at: DateTime<Utc>,
    pub topic: String,
    /// Inhalt der Nachricht, bei sehr langen Nachrichten gekürzt.
    pub payload: String,
    pub reason: String,
}

/// Eine Seite des Protokolls der verworfenen Nachrichten (neueste zuerst).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvalidMessagesPage {
    pub total: i64,
    pub items: Vec<InvalidMessage>,
}

/// Zahl der Profilbilder (Nummern 0 bis AVATAR_COUNT - 1).
pub const AVATAR_COUNT: u8 = 164;

/// Wandelt die Nummer aus der Datenbank in eine gültige Bildnummer um.
pub fn avatar_from_db(number: i16) -> u8 {
    number.clamp(0, i16::from(AVATAR_COUNT) - 1) as u8
}

/// Neues Profilbild für die angemeldete Person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetAvatar {
    pub avatar: u8,
}

/// Anmeldedaten. Mit `remember` bleibt man sehr lange angemeldet (der Browser merkt sich das Token).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub remember: bool,
}

/// Passwort ändern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangePassword {
    pub current: String,
    pub new: String,
}

pub const MIN_PASSWORD_LEN: usize = 8;
pub const MAX_PASSWORD_LEN: usize = 128;

/// Prüft ein neues Passwort: lang genug, nicht absurd lang und anders als das alte.
pub fn check_new_password(current: &str, new: &str) -> Result<(), &'static str> {
    let length = new.chars().count();
    if length < MIN_PASSWORD_LEN {
        return Err("Das neue Passwort braucht mindestens 8 Zeichen.");
    }
    if length > MAX_PASSWORD_LEN {
        return Err("Das neue Passwort darf höchstens 128 Zeichen haben.");
    }
    if new == current {
        return Err("Das neue Passwort muss sich vom alten unterscheiden.");
    }
    Ok(())
}

/// Sortierschlüssel der Geräteliste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceSort {
    #[default]
    Id,
    SocPercent,
    LastSeenAt,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_password_must_be_long_enough_and_different() {
        assert_eq!(check_new_password("alt-passwort", "neues-passwort"), Ok(()));
        assert!(check_new_password("alt-passwort", "kurz").is_err());
        assert!(check_new_password("alt-passwort", "alt-passwort").is_err());
        assert!(check_new_password("alt", &"x".repeat(MAX_PASSWORD_LEN + 1)).is_err());
        assert_eq!(check_new_password("a", &"x".repeat(MAX_PASSWORD_LEN)), Ok(()));
        assert_eq!(
            check_new_password("a", "äöüäöüäö"),
            Ok(()),
            "Zeichen zählen, nicht Bytes"
        );
        assert_eq!(AVATAR_COUNT, 164);
        assert_eq!(avatar_from_db(-3), 0);
        assert_eq!(avatar_from_db(163), 163);
        assert_eq!(avatar_from_db(500), 163);
    }

    #[test]
    fn usernames_are_short_lowercase_names() {
        for good in ["noah", "fynn.b", "lena-2", "a_b", "x1"] {
            assert_eq!(check_username(good), Ok(()), "{good}");
        }
        for bad in ["n", "Noah", "mit leerzeichen", "ä", "a/b", "", &"x".repeat(33)] {
            assert!(check_username(bad).is_err(), "{bad}");
        }
        assert_eq!(check_display_name("Noah Boufercha"), Ok(()));
        assert!(check_display_name("   ").is_err());
        assert!(check_display_name(&"x".repeat(81)).is_err());
    }

    #[test]
    fn older_login_requests_without_remember_still_work() {
        let login: Login = serde_json::from_str(r#"{"username":"noah","password":"x"}"#).unwrap();
        assert!(!login.remember);
        let me: Me = serde_json::from_str(r#"{"username":"noah","display_name":"Noah"}"#).unwrap();
        assert_eq!(me.avatar, 0);
    }

    #[test]
    fn range_roundtrips_through_text_and_json() {
        for range in Range::ALL {
            assert_eq!(range.as_str().parse::<Range>(), Ok(range));
            let json = serde_json::to_string(&range).unwrap();
            assert_eq!(json, format!("\"{}\"", range.as_str()));
            assert_eq!(serde_json::from_str::<Range>(&json).unwrap(), range);
        }
        assert!("1h".parse::<Range>().is_err());
    }

    #[test]
    fn longer_ranges_use_wider_buckets() {
        assert_eq!(Range::Day.bucket_sql(), "5 minutes");
        assert_eq!(Range::Month.window_sql(), "30 days");
    }

    #[test]
    fn sort_keys_use_snake_case() {
        assert_eq!(
            serde_json::to_string(&DeviceSort::SocPercent).unwrap(),
            "\"soc_percent\""
        );
        assert_eq!(
            serde_json::from_str::<DeviceSort>("\"last_seen_at\"").unwrap(),
            DeviceSort::LastSeenAt
        );
        assert_eq!(serde_json::from_str::<SortDir>("\"desc\"").unwrap(), SortDir::Desc);
    }
}
