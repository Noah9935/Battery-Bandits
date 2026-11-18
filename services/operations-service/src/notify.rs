//! Versand der Weiterleitungen aufs Handy.
//!
//! Die Zugänge stehen nur in der Umgebung des Servers. Ein Dienst ohne Zugang gilt als "nicht eingerichtet"
//! und wird übersprungen. Die Oberfläche zeigt das an.
//!
//! * `TELEGRAM_BOT_TOKEN`: Token eines Telegram-Bots (von BotFather). Gesendet wird über die Bot-Schnittstelle.
//! * `WHATSAPP_WEBHOOK_URL`: Adresse eines Gateways, das `{"channel","to","subject","text"}`
//!   als JSON per POST annimmt. So lässt sich jeder Anbieter anbinden, ohne dass dieser Dienst ihn kennen muss.
//!   `NOTIFY_WEBHOOK_TOKEN` wird, falls gesetzt, als Bearer-Token mitgeschickt.
//! * `SMTP_URL` (zum Beispiel `smtps://nutzer:passwort@mail.example.org:465`) und `SMTP_FROM` für E-Mails.
//! * `PUBLIC_URL`: Adresse der Oberfläche. Ist sie gesetzt, enthält jede Nachricht einen Link zum Gerät.
//!
//! Fehlertexte und Protokoll enthalten nie Zugangsdaten: Adressen mit Token werden vor der Ausgabe entfernt.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Serialize;
use serde_json::json;
use shared::Alert;
use shared::notify::{Channel, ChannelStatus, NotificationRule, alert_subject, alert_text, clean_target, with_link};
use sqlx::PgPool;

use crate::mail::{self, AlertFacts, MailContent};
use crate::repo;

const DEFAULT_TELEGRAM_API: &str = "https://api.telegram.org";

/// Zugänge der Dienste, gelesen aus der Umgebung.
#[derive(Clone, Default)]
pub struct NotifyConfig {
    pub telegram_token: Option<String>,
    pub telegram_api: Option<String>,
    pub whatsapp_webhook: Option<String>,
    pub webhook_token: Option<String>,
    pub smtp_url: Option<String>,
    pub smtp_from: Option<String>,
    pub public_url: Option<String>,
    /// Adresse von CallMeBot (Vorgabe `https://api.callmebot.com`).
    pub callmebot_url: Option<String>,
    /// WhatsApp über CallMeBot ist an. Es braucht keinen Zugang auf dem Server: Jede Person hinterlegt ihren eigenen
    /// Schlüssel. Mit `WHATSAPP_CALLMEBOT=off` abschaltbar.
    pub callmebot: bool,
}

const DEFAULT_CALLMEBOT_URL: &str = "https://api.callmebot.com";

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
    /// Liest die Zugänge über `lookup` (im Betrieb `std::env::var`). Leere Werte zählen als nicht gesetzt.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let get = |name: &str| lookup(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        Self {
            telegram_token: get("TELEGRAM_BOT_TOKEN"),
            telegram_api: get("TELEGRAM_API_URL"),
            whatsapp_webhook: get("WHATSAPP_WEBHOOK_URL"),
            webhook_token: get("NOTIFY_WEBHOOK_TOKEN"),
            smtp_url: get("SMTP_URL"),
            smtp_from: get("SMTP_FROM"),
            public_url: get("PUBLIC_URL"),
            callmebot_url: get("CALLMEBOT_URL"),
            callmebot: get("WHATSAPP_CALLMEBOT").as_deref() != Some("off"),
        }
    }

    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Welche Dienste sind eingerichtet?
    pub fn status(&self) -> ChannelStatus {
        ChannelStatus {
            whatsapp: self.whatsapp_webhook.is_some() || self.callmebot,
            telegram: self.telegram_token.is_some(),
            email: self.smtp_url.is_some() && self.smtp_from.is_some(),
        }
    }
}

/// Ergebnis eines Sendeversuchs, so wie es die Oberfläche beim Test anzeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Delivery {
    pub channel: Channel,
    pub ok: bool,
    pub detail: String,
}

/// Sendet Nachrichten über die eingerichteten Dienste.
#[derive(Clone)]
pub struct Notifier {
    config: Arc<NotifyConfig>,
    http: reqwest::Client,
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
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            mailer,
        }
    }

    /// Ohne Zugänge: nichts wird gesendet (Tests, Entwicklung).
    pub fn disabled() -> Self {
        Self::new(NotifyConfig::default())
    }

    pub fn status(&self) -> ChannelStatus {
        let mut status = self.config.status();
        status.email = status.email && self.mailer.is_some();
        status
    }

    pub fn public_url(&self) -> Option<&str> {
        self.config.public_url.as_deref()
    }

    /// Sendet einen Text über einen Dienst. Der Fehlertext ist für Menschen gedacht und frei von Zugangsdaten.
    pub async fn send(&self, channel: Channel, target: &str, subject: &str, text: &str) -> Result<(), String> {
        self.send_with(channel, target, "", subject, text).await
    }

    /// Wie [`Notifier::send`], mit dem persönlichen Schlüssel der Person (für WhatsApp über CallMeBot).
    pub async fn send_with(
        &self,
        channel: Channel,
        target: &str,
        key: &str,
        subject: &str,
        text: &str,
    ) -> Result<(), String> {
        let Some(target) = clean_target(channel, target) else {
            return Err(format!("{} ist nicht gültig.", channel.target_label()));
        };
        match channel {
            Channel::Telegram => self.send_telegram(&target, text).await,
            Channel::WhatsApp => self.send_whatsapp(&target, key.trim(), subject, text).await,
            Channel::Email => self.send_email(&target, subject, text).await,
        }
    }

    async fn send_telegram(&self, chat_id: &str, text: &str) -> Result<(), String> {
        let Some(token) = &self.config.telegram_token else {
            return Err(not_configured(Channel::Telegram));
        };
        let api = self
            .config
            .telegram_api
            .as_deref()
            .unwrap_or(DEFAULT_TELEGRAM_API)
            .trim_end_matches('/');
        let request = self
            .http
            .post(format!("{api}/bot{token}/sendMessage"))
            .json(&json!({"chat_id": chat_id, "text": text, "disable_web_page_preview": true}));
        check(request.send().await, "Telegram").await
    }

    /// WhatsApp ohne Anbieter-Vertrag: Mit dem Schlüssel der Person schickt CallMeBot die Nachricht an ihre eigene
    /// Nummer. Ist ein Gateway eingerichtet und die Person hat keinen Schlüssel, geht es darüber. Der Schlüssel taucht
    /// in keiner Fehlermeldung und in keinem Protokoll auf.
    async fn send_whatsapp(&self, phone: &str, key: &str, subject: &str, text: &str) -> Result<(), String> {
        if !key.is_empty() && self.config.callmebot {
            let base = self
                .config
                .callmebot_url
                .as_deref()
                .unwrap_or(DEFAULT_CALLMEBOT_URL)
                .trim_end_matches('/');
            let request = self.http.get(format!("{base}/whatsapp.php")).query(&[
                ("phone", format!("+{phone}")),
                ("text", text.to_owned()),
                ("apikey", key.to_owned()),
            ]);
            return check(request.send().await, "CallMeBot").await;
        }
        if self.config.whatsapp_webhook.is_some() {
            return self
                .send_webhook(
                    self.config.whatsapp_webhook.as_deref(),
                    Channel::WhatsApp,
                    phone,
                    subject,
                    text,
                )
                .await;
        }
        if !self.config.callmebot {
            return Err(not_configured(Channel::WhatsApp));
        }
        Err(
            "WhatsApp: Der CallMeBot-Schlüssel fehlt. Einmalig bei CallMeBot freischalten und hier eintragen."
                .to_owned(),
        )
    }

    async fn send_webhook(
        &self,
        url: Option<&str>,
        channel: Channel,
        to: &str,
        subject: &str,
        text: &str,
    ) -> Result<(), String> {
        let Some(url) = url else {
            return Err(not_configured(channel));
        };
        let mut request = self
            .http
            .post(url)
            .json(&json!({"channel": channel.as_str(), "to": to, "subject": subject, "text": text}));
        if let Some(token) = &self.config.webhook_token {
            request = request.bearer_auth(token);
        }
        check(request.send().await, "Das Gateway").await
    }

    async fn send_email(&self, to: &str, subject: &str, text: &str) -> Result<(), String> {
        let (Some(mailer), Some(from)) = (&self.mailer, &self.config.smtp_from) else {
            return Err(not_configured(Channel::Email));
        };
        let from: Mailbox = from
            .parse()
            .map_err(|_| "Der Absender (SMTP_FROM) ist nicht gültig.".to_owned())?;
        let to: Mailbox = to
            .parse()
            .map_err(|_| "Die E-Mail-Adresse ist nicht gültig.".to_owned())?;
        let message = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .body(text.to_owned())
            .map_err(|_| "Die E-Mail konnte nicht zusammengestellt werden.".to_owned())?;
        mailer
            .send(message)
            .await
            .map(|_| ())
            .map_err(|_| "Der Mailserver hat die Nachricht nicht angenommen.".to_owned())
    }

    /// Sendet eine gestaltete E-Mail (Text und HTML mit eingebettetem Logo, siehe [`mail`]).
    async fn send_mail(&self, target: &str, content: &MailContent) -> Result<(), String> {
        let Some(to) = clean_target(Channel::Email, target) else {
            return Err(format!("{} ist nicht gültig.", Channel::Email.target_label()));
        };
        let (Some(mailer), Some(from)) = (&self.mailer, &self.config.smtp_from) else {
            return Err(not_configured(Channel::Email));
        };
        let message = build_message(from, &to, content)?;
        mailer
            .send(message)
            .await
            .map(|_| ())
            .map_err(|_| "Der Mailserver hat die Nachricht nicht angenommen.".to_owned())
    }

    /// Sendet eine neue Meldung an alle Personen, deren Regel sie verlangt, über jeden dort eingeschalteten und
    /// auf dem Server eingerichteten Dienst. Jeder Versuch wird protokolliert. Gibt die Zahl der Erfolge zurück.
    pub async fn deliver_alert(&self, pool: &PgPool, alert: &Alert) -> usize {
        let status = self.status();
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
            .filter(|(user, _)| recipients.contains(user))
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
        let subject = alert_subject(&alert.device_id, alert.kind);
        let text = with_link(
            &alert_text(
                &alert.device_id,
                device.name.as_deref(),
                alert.kind,
                device.soc_percent,
                device.soc_sigma_percent,
            ),
            self.public_url(),
            Some(&alert.device_id),
        );
        // Die E-Mail ist gestaltet und für alle Empfänger gleich, deshalb wird sie nur einmal gebaut.
        let email = mail::render_alert(&AlertFacts {
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
            for channel in rule.channels_for(alert.kind) {
                if !status.ready(channel) {
                    continue;
                }
                let result = if channel == Channel::Email {
                    self.send_mail(rule.target(channel), &email).await
                } else {
                    self.send_with(channel, rule.target(channel), rule.whatsapp_key(), &subject, &text)
                        .await
                };
                let (ok, detail) = match &result {
                    Ok(()) => (true, "gesendet"),
                    Err(e) => (false, e.as_str()),
                };
                if ok {
                    sent += 1;
                } else {
                    tracing::warn!(user = %username, channel = channel.as_str(), "Weiterleitung fehlgeschlagen: {detail}");
                }
                if let Err(e) = repo::log_notification(pool, &username, channel, Some(alert.id), ok, detail).await {
                    tracing::warn!("Weiterleitung nicht protokolliert: {e}");
                }
            }
        }
        sent
    }

    /// Probenachricht über alle in der Regel eingeschalteten Dienste, damit man die Einrichtung prüfen kann.
    pub async fn send_test(&self, pool: &PgPool, username: &str, rule: &NotificationRule) -> Vec<Delivery> {
        let status = self.status();
        let text = with_link(
            "Battery Bandits: Probenachricht. Die Weiterleitung funktioniert.",
            self.public_url(),
            None,
        );
        let mut results = Vec::new();
        for channel in Channel::ALL.into_iter().filter(|c| rule.is_on(*c)) {
            let result = if status.ready(channel) && channel == Channel::Email {
                self.send_mail(rule.target(channel), &mail::render_test(self.public_url()))
                    .await
            } else if status.ready(channel) {
                self.send_with(
                    channel,
                    rule.target(channel),
                    rule.whatsapp_key(),
                    "Battery Bandits: Probenachricht",
                    &text,
                )
                .await
            } else {
                Err(not_configured(channel))
            };
            let (ok, detail) = match result {
                Ok(()) => (true, "gesendet".to_owned()),
                Err(e) => (false, e),
            };
            let _ = repo::log_notification(pool, username, channel, None, ok, &detail).await;
            results.push(Delivery { channel, ok, detail });
        }
        results
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

fn not_configured(channel: Channel) -> String {
    format!("{} ist auf dem Server nicht eingerichtet.", channel.label())
}

/// Wertet die Antwort eines Dienstes aus. Die Adresse der Anfrage (mit Token) taucht im Fehler nie auf.
async fn check(result: Result<reqwest::Response, reqwest::Error>, who: &str) -> Result<(), String> {
    match result {
        Ok(response) if response.status().is_success() => Ok(()),
        Ok(response) => Err(format!(
            "{who} hat mit Status {} abgelehnt.",
            response.status().as_u16()
        )),
        Err(e) if e.is_timeout() => Err(format!("{who} hat nicht rechtzeitig geantwortet.")),
        Err(_) => Err(format!("{who} ist nicht erreichbar.")),
    }
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
    fn nothing_is_ready_without_credentials() {
        let none = config(&[("WHATSAPP_CALLMEBOT", "off")]);
        assert_eq!(none.status(), ChannelStatus::default());
        // WhatsApp über CallMeBot braucht keinen Zugang auf dem Server (jede Person bringt ihren Schlüssel mit).
        let default = config(&[]);
        assert_eq!(
            default.status(),
            ChannelStatus {
                whatsapp: true,
                ..ChannelStatus::default()
            }
        );
        assert_eq!(Notifier::disabled().status(), ChannelStatus::default());
        assert_eq!(Notifier::disabled().public_url(), None);
    }

    #[test]
    fn each_credential_enables_its_channel() {
        let c = config(&[
            ("TELEGRAM_BOT_TOKEN", " 123:abc "),
            ("WHATSAPP_WEBHOOK_URL", "   "),
            ("WHATSAPP_CALLMEBOT", "off"),
            ("SMTP_URL", "smtp://localhost:25"),
        ]);
        let status = c.status();
        assert!(status.telegram);
        assert!(!status.whatsapp, "leere Werte zählen nicht");
        assert!(!status.email, "ohne Absender keine E-Mail");
        assert_eq!(c.telegram_token.as_deref(), Some("123:abc"));

        let with_from = config(&[("SMTP_URL", "smtp://localhost:25"), ("SMTP_FROM", "alarm@beispiel.de")]);
        assert!(with_from.status().email);
    }

    #[test]
    fn debug_output_never_contains_secrets() {
        let c = config(&[
            ("TELEGRAM_BOT_TOKEN", "geheim-123"),
            ("SMTP_URL", "smtps://nutzer:passwort@mail.example.org"),
            ("SMTP_FROM", "alarm@beispiel.de"),
            ("NOTIFY_WEBHOOK_TOKEN", "auch-geheim"),
        ]);
        let text = format!("{c:?}");
        assert!(!text.contains("geheim") && !text.contains("passwort"), "{text}");
        assert!(text.contains("telegram: true"));
    }

    #[tokio::test]
    async fn an_unreadable_smtp_address_disables_email_instead_of_crashing() {
        let notifier = Notifier::new(config(&[("SMTP_URL", "kein-url"), ("SMTP_FROM", "alarm@beispiel.de")]));
        assert!(!notifier.status().email);
        assert_eq!(
            notifier
                .send(Channel::Email, "noah@beispiel.de", "s", "t")
                .await
                .unwrap_err(),
            "E-Mail ist auf dem Server nicht eingerichtet."
        );
    }

    #[tokio::test]
    async fn sending_without_setup_or_with_a_bad_target_explains_itself() {
        let n = Notifier::disabled();
        assert_eq!(
            n.send(Channel::Telegram, "123456789", "s", "t").await.unwrap_err(),
            "Telegram ist auf dem Server nicht eingerichtet."
        );
        assert_eq!(
            n.send(Channel::WhatsApp, "+49 170 1234567", "s", "t")
                .await
                .unwrap_err(),
            "WhatsApp ist auf dem Server nicht eingerichtet."
        );
        assert_eq!(
            n.send(Channel::WhatsApp, "quatsch", "s", "t").await.unwrap_err(),
            "Handynummer mit Ländervorwahl ist nicht gültig."
        );
    }
}
