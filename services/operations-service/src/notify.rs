//! Versand der Weiterleitungen per E-Mail.
//!
//! Der Zugang steht nur in der Umgebung des Servers. Ohne ihn gilt der Versand als "nicht eingerichtet" und wird
//! übersprungen. Die Oberfläche zeigt das an.
//!
//! * `SMTP_URL` (zum Beispiel `smtps://nutzer:passwort@mail.example.org:465`) und `SMTP_FROM`.
//! * `PUBLIC_URL`: Adresse der Oberfläche. Ist sie gesetzt, enthält jede Mail einen Link zum Gerät.
//!
//! Fehlertexte und Protokoll enthalten nie Zugangsdaten.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Serialize;
use shared::Alert;
use shared::notify::{MailStatus, NotificationRule, clean_email};
use sqlx::PgPool;

use crate::mail::{self, AlertFacts, MailContent};
use crate::repo;

const NOT_CONFIGURED: &str = "E-Mail ist auf dem Server nicht eingerichtet.";

/// Zugang zum Mailserver, gelesen aus der Umgebung.
#[derive(Clone, Default)]
pub struct NotifyConfig {
    pub smtp_url: Option<String>,
    pub smtp_from: Option<String>,
    pub public_url: Option<String>,
}

// Eigene Ausgabe, damit Zugangsdaten nie in einem Protokoll landen.
impl std::fmt::Debug for NotifyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyConfig")
            .field("status", &self.status())
            .field("public_url", &self.public_url)
            .finish_non_exhaustive()
    }
}

impl NotifyConfig {
    /// Liest die Werte über `lookup` (im Betrieb `std::env::var`). Leere Werte zählen als nicht gesetzt.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str| lookup(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        Self {
            smtp_url: get("SMTP_URL"),
            smtp_from: get("SMTP_FROM"),
            public_url: get("PUBLIC_URL"),
        }
    }

    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Ist der Zugang zum Mailserver eingetragen?
    pub fn status(&self) -> MailStatus {
        MailStatus {
            ready: self.smtp_url.is_some() && self.smtp_from.is_some(),
        }
    }
}

/// Ergebnis eines Sendeversuchs, so wie es die Oberfläche beim Test anzeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Delivery {
    pub ok: bool,
    pub detail: String,
}

/// Sendet Mails über den eingerichteten Mailserver.
#[derive(Clone)]
pub struct Notifier {
    config: Arc<NotifyConfig>,
    mailer: Option<AsyncSmtpTransport<Tokio1Executor>>,
}

impl Notifier {
    pub fn new(config: NotifyConfig) -> Self {
        let mailer = match (&config.smtp_url, &config.smtp_from) {
            (Some(url), Some(_)) => match AsyncSmtpTransport::<Tokio1Executor>::from_url(url) {
                Ok(builder) => Some(builder.timeout(Some(Duration::from_secs(15))).build()),
                Err(_) => {
                    // Die Adresse enthält das Passwort und wird deshalb nicht ausgegeben.
                    tracing::warn!("SMTP_URL ist nicht lesbar, E-Mails werden nicht gesendet");
                    None
                }
            },
            _ => None,
        };
        Self {
            config: Arc::new(config),
            mailer,
        }
    }

    /// Ohne Zugang: nichts wird gesendet (Tests, Entwicklung).
    pub fn disabled() -> Self {
        Self::new(NotifyConfig::default())
    }

    /// Kann der Server Mails senden? Nur wenn der Zugang eingetragen ist und sich lesen lässt.
    pub fn status(&self) -> MailStatus {
        MailStatus {
            ready: self.config.status().ready && self.mailer.is_some(),
        }
    }

    pub fn public_url(&self) -> Option<&str> {
        self.config.public_url.as_deref()
    }

    /// Sendet eine gestaltete E-Mail (Text und HTML mit eingebettetem Logo, siehe [`mail`]). Der Fehlertext ist für
    /// Menschen gedacht und frei von Zugangsdaten.
    pub async fn send_mail(&self, target: &str, content: &MailContent) -> Result<(), String> {
        let Some(to) = clean_email(target) else {
            return Err("Die E-Mail-Adresse ist nicht gültig.".to_owned());
        };
        let (Some(mailer), Some(from)) = (&self.mailer, &self.config.smtp_from) else {
            return Err(NOT_CONFIGURED.to_owned());
        };
        let message = build_message(from, &to, content)?;
        mailer
            .send(message)
            .await
            .map(|_| ())
            .map_err(|_| "Der Mailserver hat die Nachricht nicht angenommen.".to_owned())
    }

    /// Sendet eine neue Meldung an alle Personen, deren Regel sie verlangt. Jeder Versuch wird protokolliert. Gibt die
    /// Zahl der Erfolge zurück.
    pub async fn deliver_alert(&self, pool: &PgPool, alert: &Alert) -> usize {
        if !self.status().ready {
            return 0;
        }
        let rules = match repo::active_notification_rules(pool).await {
            Ok(rules) => rules,
            Err(e) => {
                tracing::warn!("Weiterleitungsregeln nicht lesbar: {e}");
                return 0;
            }
        };
        if rules.is_empty() {
            return 0;
        }
        // Nur wer die Meldung bekommt: die Eingetragenen des Nodes, sonst alle.
        let recipients: std::collections::HashSet<String> = match repo::alert_recipients(pool, &alert.device_id).await {
            Ok(list) => list.into_iter().collect(),
            Err(e) => {
                tracing::warn!("Empfänger nicht lesbar: {e}");
                return 0;
            }
        };
        let rules: Vec<_> = rules
            .into_iter()
            .filter(|(user, rule)| recipients.contains(user) && rule.sends(alert.kind))
            .collect();
        if rules.is_empty() {
            return 0;
        }
        let device: DeviceFacts = sqlx::query_as(
            "select name, soc_percent, soc_sigma_percent, voltage_mv, last_seen_at, forecast_hours_remaining, \
             forecast_hours_low, forecast_hours_high, lat, lon \
             from devices where id = $1",
        )
        .bind(&alert.device_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
        // Die Mail ist gestaltet und für alle Empfänger gleich, deshalb wird sie nur einmal gebaut.
        let content = mail::render_alert(&AlertFacts {
            device_id: &alert.device_id,
            name: device.name.as_deref(),
            kind: alert.kind,
            soc_percent: device.soc_percent,
            soc_sigma_percent: device.soc_sigma_percent,
            voltage_mv: device.voltage_mv,
            last_seen: device.last_seen_at,
            forecast_hours: device.forecast_hours_remaining,
            forecast_hours_low: device.forecast_hours_low,
            forecast_hours_high: device.forecast_hours_high,
            position: device.lat.zip(device.lon),
            base_url: self.public_url(),
            now: Utc::now(),
        });
        let mut sent = 0;
        for (username, rule) in rules {
            let result = self.send_mail(&rule.email_address, &content).await;
            let (ok, detail) = match &result {
                Ok(()) => (true, "gesendet"),
                Err(e) => (false, e.as_str()),
            };
            if ok {
                sent += 1;
            } else {
                tracing::warn!(user = %username, "Weiterleitung fehlgeschlagen: {detail}");
            }
            if let Err(e) = repo::log_notification(pool, &username, Some(alert.id), ok, detail).await {
                tracing::warn!("Weiterleitung nicht protokolliert: {e}");
            }
        }
        sent
    }

    /// Probemail an die gespeicherte Adresse, damit man die Einrichtung prüfen kann.
    pub async fn send_test(&self, pool: &PgPool, username: &str, rule: &NotificationRule) -> Delivery {
        let result = if !self.status().ready {
            Err(NOT_CONFIGURED.to_owned())
        } else if rule.email_address.trim().is_empty() {
            Err("Zum Testen die E-Mail-Adresse eintragen und speichern.".to_owned())
        } else {
            self.send_mail(&rule.email_address, &mail::render_test(self.public_url()))
                .await
        };
        let (ok, detail) = match result {
            Ok(()) => (true, "gesendet".to_owned()),
            Err(e) => (false, e),
        };
        let _ = repo::log_notification(pool, username, None, ok, &detail).await;
        Delivery { ok, detail }
    }
}

/// Angaben zum Gerät, die in die Mail zur Meldung kommen.
#[derive(Default, sqlx::FromRow)]
struct DeviceFacts {
    name: Option<String>,
    soc_percent: Option<f32>,
    soc_sigma_percent: Option<f32>,
    voltage_mv: Option<i32>,
    last_seen_at: Option<DateTime<Utc>>,
    forecast_hours_remaining: Option<f32>,
    forecast_hours_low: Option<f32>,
    forecast_hours_high: Option<f32>,
    lat: Option<f64>,
    lon: Option<f64>,
}

/// Baut die Nachricht: Text und HTML als Alternativen, das Logo als eingebettetes Bild am HTML. Steht in `from` nur
/// eine Adresse, wird "Battery Bandits" als Absendername davorgesetzt.
pub fn build_message(from: &str, to: &str, content: &MailContent) -> Result<Message, String> {
    let mut from: Mailbox = from
        .parse()
        .map_err(|_| "Der Absender (SMTP_FROM) ist nicht gültig.".to_owned())?;
    if from.name.is_none() {
        from = Mailbox::new(Some(mail::SENDER_NAME.to_owned()), from.email);
    }
    let to: Mailbox = to
        .parse()
        .map_err(|_| "Die E-Mail-Adresse ist nicht gültig.".to_owned())?;
    let logo = Attachment::new_inline(mail::LOGO_CID.to_owned()).body(
        mail::LOGO_PNG.to_vec(),
        ContentType::parse("image/png").map_err(|_| "Das Logo hat keinen gültigen Typ.".to_owned())?,
    );
    let body = MultiPart::alternative()
        .singlepart(SinglePart::plain(content.text.clone()))
        .multipart(
            MultiPart::related()
                .singlepart(SinglePart::html(content.html.clone()))
                .singlepart(logo),
        );
    Message::builder()
        .from(from)
        .to(to)
        .subject(content.subject.clone())
        .multipart(body)
        .map_err(|_| "Die E-Mail konnte nicht zusammengestellt werden.".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> NotifyConfig {
        let map: std::collections::HashMap<String, String> =
            pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        NotifyConfig::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn the_email_has_text_html_and_the_logo_and_a_sender_name() {
        let content = mail::render_test(Some("https://bandits.example.org"));
        let message = build_message("alarm@example.org", "noah@example.org", &content).unwrap();
        let raw = String::from_utf8_lossy(&message.formatted()).into_owned();
        for expected in [
            "multipart/alternative",
            "multipart/related",
            "text/plain",
            "text/html",
            "Content-Type: image/png",
            "Content-ID: <logo>",
            "noah@example.org",
        ] {
            assert!(raw.contains(expected), "{expected} fehlt:\n{raw}");
        }
        // Wie der Name geschrieben wird (mit oder ohne Anführungszeichen), entscheidet lettre.
        let from_line = |raw: &str| {
            raw.lines()
                .find(|line| line.starts_with("From:"))
                .unwrap_or_default()
                .to_owned()
        };
        let line = from_line(&raw);
        assert!(
            line.contains("Battery Bandits") && line.contains("<alarm@example.org>"),
            "{line}"
        );
        let named = build_message("Alarm <alarm@example.org>", "noah@example.org", &content).unwrap();
        let line = from_line(&String::from_utf8_lossy(&named.formatted()));
        assert!(line.contains("Alarm") && !line.contains("Battery Bandits"), "{line}");
        assert!(build_message("kein-absender", "noah@example.org", &content).is_err());
        assert!(build_message("alarm@example.org", "keine adresse", &content).is_err());
    }

    #[test]
    fn nothing_is_ready_without_the_mail_server_access() {
        assert!(!config(&[]).status().ready);
        assert!(
            !config(&[("SMTP_URL", "smtp://localhost:25")]).status().ready,
            "ohne Absender keine E-Mail"
        );
        assert!(
            !config(&[("SMTP_URL", "   "), ("SMTP_FROM", "alarm@beispiel.de")])
                .status()
                .ready,
            "leere Werte zählen nicht"
        );
        assert!(
            config(&[("SMTP_URL", "smtp://localhost:25"), ("SMTP_FROM", "alarm@beispiel.de")])
                .status()
                .ready
        );
        assert!(!Notifier::disabled().status().ready);
        assert_eq!(Notifier::disabled().public_url(), None);
    }

    #[test]
    fn debug_output_never_contains_secrets() {
        let c = config(&[
            ("SMTP_URL", "smtps://nutzer:passwort@mail.example.org"),
            ("SMTP_FROM", "alarm@beispiel.de"),
        ]);
        let text = format!("{c:?}");
        assert!(!text.contains("passwort") && !text.contains("nutzer"), "{text}");
        assert!(text.contains("ready: true"), "{text}");
    }

    #[tokio::test]
    async fn an_unreadable_smtp_address_disables_email_instead_of_crashing() {
        let notifier = Notifier::new(config(&[("SMTP_URL", "kein-url"), ("SMTP_FROM", "alarm@beispiel.de")]));
        assert!(!notifier.status().ready);
        assert_eq!(
            notifier
                .send_mail("noah@beispiel.de", &mail::render_test(None))
                .await
                .unwrap_err(),
            "E-Mail ist auf dem Server nicht eingerichtet."
        );
    }

    #[tokio::test]
    async fn sending_without_setup_or_with_a_bad_address_explains_itself() {
        let n = Notifier::disabled();
        let content = mail::render_test(None);
        assert_eq!(
            n.send_mail("noah@beispiel.de", &content).await.unwrap_err(),
            "E-Mail ist auf dem Server nicht eingerichtet."
        );
        assert_eq!(
            n.send_mail("quatsch", &content).await.unwrap_err(),
            "Die E-Mail-Adresse ist nicht gültig."
        );
    }
}
