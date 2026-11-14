//! Weiterleitung von Meldungen aufs Handy: Dienste, Regel je Person, Texte und Links.
//!
//! Zwei Wege:
//! * Von Hand: Ein Link öffnet WhatsApp, Telegram, SMS oder das Mailprogramm mit vorbereitetem Text
//!   ([`share_url`]). Das braucht keine Einrichtung auf dem Server.
//! * Automatisch: Der Meldungs- und Einsatz-Service sendet neue Meldungen nach der Regel der Person
//!   ([`NotificationRule`]). Dafür müssen die Zugänge der Dienste auf dem Server hinterlegt sein.

use serde::{Deserialize, Serialize};

use crate::api::encode_header;
use crate::models::AlertKind;

/// Ein Dienst, über den Meldungen aufs Handy gehen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    WhatsApp,
    Telegram,
    Sms,
    Email,
}

impl Channel {
    pub const ALL: [Channel; 4] = [Channel::WhatsApp, Channel::Telegram, Channel::Sms, Channel::Email];

    /// Kennung für Datenbank und Schnittstelle.
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::WhatsApp => "whatsapp",
            Channel::Telegram => "telegram",
            Channel::Sms => "sms",
            Channel::Email => "email",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Channel::WhatsApp => "WhatsApp",
            Channel::Telegram => "Telegram",
            Channel::Sms => "SMS",
            Channel::Email => "E-Mail",
        }
    }

    /// Was die Person für diesen Dienst angeben muss.
    pub fn target_label(self) -> &'static str {
        match self {
            Channel::WhatsApp | Channel::Sms => "Handynummer mit Ländervorwahl",
            Channel::Telegram => "Chat-ID",
            Channel::Email => "E-Mail-Adresse",
        }
    }

    pub fn target_placeholder(self) -> &'static str {
        match self {
            Channel::WhatsApp | Channel::Sms => "+49 170 1234567",
            Channel::Telegram => "123456789",
            Channel::Email => "name@beispiel.de",
        }
    }
}

/// Welche Dienste der Server gerade bedienen kann (Zugänge sind in seiner Umgebung hinterlegt).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub whatsapp: bool,
    pub telegram: bool,
    pub sms: bool,
    pub email: bool,
}

impl ChannelStatus {
    pub fn ready(&self, channel: Channel) -> bool {
        match channel {
            Channel::WhatsApp => self.whatsapp,
            Channel::Telegram => self.telegram,
            Channel::Sms => self.sms,
            Channel::Email => self.email,
        }
    }
}

/// Regel einer Person: Welche Meldungen gehen automatisch über welche Dienste?
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct NotificationRule {
    /// Hauptschalter. Ist er aus, wird nichts gesendet.
    pub enabled: bool,
    /// Nur dringende Meldungen (kritisch, offline, ausgefallen), keine Auffälligkeiten.
    pub only_critical: bool,
    pub whatsapp: bool,
    pub whatsapp_number: String,
    /// Schlüssel des persönlichen CallMeBot (kostenloser Dienst, der Nachrichten an die eigene Nummer schickt). Er
    /// gehört der Person und wird nur ihr selbst angezeigt.
    #[serde(default)]
    pub whatsapp_key: String,
    pub telegram: bool,
    pub telegram_chat_id: String,
    pub sms: bool,
    pub sms_number: String,
    pub email: bool,
    pub email_address: String,
}

impl NotificationRule {
    pub fn is_on(&self, channel: Channel) -> bool {
        match channel {
            Channel::WhatsApp => self.whatsapp,
            Channel::Telegram => self.telegram,
            Channel::Sms => self.sms,
            Channel::Email => self.email,
        }
    }

    pub fn set_on(&mut self, channel: Channel, on: bool) {
        match channel {
            Channel::WhatsApp => self.whatsapp = on,
            Channel::Telegram => self.telegram = on,
            Channel::Sms => self.sms = on,
            Channel::Email => self.email = on,
        }
    }

    /// Ziel beim Dienst (Nummer, Chat-ID oder Adresse), so wie es eingegeben wurde.
    pub fn target(&self, channel: Channel) -> &str {
        match channel {
            Channel::WhatsApp => &self.whatsapp_number,
            Channel::Telegram => &self.telegram_chat_id,
            Channel::Sms => &self.sms_number,
            Channel::Email => &self.email_address,
        }
    }

    /// Schlüssel für WhatsApp über CallMeBot, ohne Leerzeichen am Rand.
    pub fn whatsapp_key(&self) -> &str {
        self.whatsapp_key.trim()
    }

    pub fn set_target(&mut self, channel: Channel, value: &str) {
        let value = value.trim().to_owned();
        match channel {
            Channel::WhatsApp => self.whatsapp_number = value,
            Channel::Telegram => self.telegram_chat_id = value,
            Channel::Sms => self.sms_number = value,
            Channel::Email => self.email_address = value,
        }
    }

    /// Soll eine Meldung dieser Art überhaupt gesendet werden?
    pub fn wants(&self, kind: AlertKind) -> bool {
        self.enabled && (!self.only_critical || kind.is_urgent())
    }

    /// Dienste, über die eine Meldung dieser Art geht: eingeschaltet und mit gültigem Ziel.
    pub fn channels_for(&self, kind: AlertKind) -> Vec<Channel> {
        if !self.wants(kind) {
            return Vec::new();
        }
        Channel::ALL
            .into_iter()
            .filter(|c| self.is_on(*c) && clean_target(*c, self.target(*c)).is_some())
            .collect()
    }

    /// Prüft die Eingaben. Ein eingeschalteter Dienst braucht ein gültiges Ziel, ein ausgeschalteter
    /// darf leer bleiben, ein ausgefülltes Ziel muss aber stimmen.
    pub fn validate(&self) -> Result<(), String> {
        for channel in Channel::ALL {
            let target = self.target(channel).trim();
            if target.is_empty() {
                if self.is_on(channel) {
                    return Err(format!("{}: {} fehlt.", channel.label(), channel.target_label()));
                }
                continue;
            }
            if clean_target(channel, target).is_none() {
                return Err(format!(
                    "{}: {} ist nicht gültig.",
                    channel.label(),
                    channel.target_label()
                ));
            }
        }
        Ok(())
    }
}

/// Handynummer in der Schreibweise der Dienste: nur Ziffern mit führender Ländervorwahl, ohne Plus.
/// Erlaubt sind Leerzeichen, Bindestriche, Schrägstriche und Klammern als Trennzeichen sowie `+` oder `00` vorn.
/// Eine Nummer mit einer einzelnen führenden Null gilt als deutsche Nummer.
pub fn clean_phone(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed
        .chars()
        .any(|c| !(c.is_ascii_digit() || matches!(c, '+' | ' ' | '-' | '/' | '(' | ')')))
    {
        return None;
    }
    if trimmed.chars().skip(1).any(|c| c == '+') {
        return None;
    }
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    let international = if trimmed.starts_with('+') {
        digits
    } else if let Some(rest) = digits.strip_prefix("00") {
        rest.to_owned()
    } else if let Some(rest) = digits.strip_prefix('0') {
        format!("49{rest}")
    } else {
        digits
    };
    ((8..=15).contains(&international.len()) && !international.starts_with('0')).then_some(international)
}

/// Chat-ID bei Telegram: eine Zahl (bei Gruppen negativ) oder ein Kanalname mit `@`.
pub fn clean_chat_id(input: &str) -> Option<String> {
    let id = input.trim();
    let number = id.strip_prefix('-').unwrap_or(id);
    let is_number = (5..=16).contains(&number.len()) && number.chars().all(|c| c.is_ascii_digit());
    let is_name = id.strip_prefix('@').is_some_and(|name| {
        (5..=32).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    });
    (is_number || is_name).then(|| id.to_owned())
}

/// E-Mail-Adresse, grob geprüft: ein `@`, davor und danach etwas, ein Punkt in der Domain, keine Leerzeichen
/// oder Zeichen, mit denen sich Kopfzeilen einschleusen lassen.
pub fn clean_email(input: &str) -> Option<String> {
    let address = input.trim();
    if address.len() > 254
        || address
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ',' | ';' | '"'))
    {
        return None;
    }
    let (local, domain) = address.split_once('@')?;
    let domain_ok = domain.contains('.')
        && !domain.contains('@')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..");
    (!local.is_empty() && domain_ok).then(|| address.to_owned())
}

/// Ziel in der Form, die der Dienst erwartet, oder `None`, wenn die Eingabe nicht stimmt.
pub fn clean_target(channel: Channel, input: &str) -> Option<String> {
    match channel {
        Channel::WhatsApp | Channel::Sms => clean_phone(input),
        Channel::Telegram => clean_chat_id(input),
        Channel::Email => clean_email(input),
    }
}

/// Text einer Meldung für das Handy. Kurz, weil SMS nach 160 Zeichen geteilt werden.
pub fn alert_text(device_id: &str, device_name: Option<&str>, kind: AlertKind, soc_percent: Option<f32>) -> String {
    let name = device_name.map(str::trim).filter(|n| !n.is_empty());
    let device = match name {
        Some(name) => format!("{device_id} ({name})"),
        None => device_id.to_owned(),
    };
    let charge = match soc_percent {
        Some(soc) if matches!(kind, AlertKind::Critical | AlertKind::Drop) => {
            format!(" Ladestand {} %.", format!("{soc:.0}").replace('.', ","))
        }
        _ => String::new(),
    };
    format!("Battery Bandits: {device}, {}.{charge}", kind.label())
}

/// Betreff für E-Mails.
pub fn alert_subject(device_id: &str, kind: AlertKind) -> String {
    format!("Battery Bandits: {device_id}, {}", kind.label())
}

/// Text mit Link zum Gerät, falls die Adresse der Oberfläche bekannt ist.
pub fn with_link(text: &str, base_url: Option<&str>, device_id: Option<&str>) -> String {
    let base = base_url
        .map(|b| b.trim().trim_end_matches('/'))
        .filter(|b| !b.is_empty());
    match (base, device_id) {
        (Some(base), Some(id)) => format!("{text} {base}/geraete/{}", encode_header(id)),
        (Some(base), None) => format!("{text} {base}"),
        _ => text.to_owned(),
    }
}

/// Link, der den Dienst auf dem Handy mit vorbereitetem Text öffnet. `target` ist das hinterlegte Ziel
/// (Nummer oder Adresse). Fehlt es oder ist es ungültig, wählt man den Empfänger im Dienst selbst.
/// `link` ist die Adresse, auf die der Text zeigt (bei Telegram ein eigenes Feld).
pub fn share_url(channel: Channel, target: Option<&str>, subject: &str, text: &str, link: Option<&str>) -> String {
    let target = target.and_then(|t| clean_target(channel, t));
    let full = match link {
        Some(link) if !link.is_empty() => format!("{text} {link}"),
        _ => text.to_owned(),
    };
    match channel {
        Channel::WhatsApp => format!(
            "https://wa.me/{}?text={}",
            target.unwrap_or_default(),
            encode_header(&full)
        ),
        Channel::Telegram => match link {
            Some(link) if !link.is_empty() => format!(
                "https://t.me/share/url?url={}&text={}",
                encode_header(link),
                encode_header(text)
            ),
            // Ohne Link steht der Text im Pflichtfeld `url`, sonst zeigt Telegram nichts an.
            _ => format!("https://t.me/share/url?url={}", encode_header(text)),
        },
        // `?&body=` verstehen sowohl iOS als auch Android.
        Channel::Sms => format!(
            "sms:{}?&body={}",
            target.map(|t| format!("+{t}")).unwrap_or_default(),
            encode_header(&full)
        ),
        Channel::Email => format!(
            "mailto:{}?subject={}&body={}",
            target.unwrap_or_default(),
            encode_header(subject),
            encode_header(&full)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> NotificationRule {
        NotificationRule {
            enabled: true,
            only_critical: true,
            telegram: true,
            telegram_chat_id: "123456789".into(),
            email: true,
            email_address: "noah@beispiel.de".into(),
            ..NotificationRule::default()
        }
    }

    #[test]
    fn channels_have_ids_labels_and_hints() {
        let ids: std::collections::BTreeSet<&str> = Channel::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(ids.len(), 4);
        for channel in Channel::ALL {
            assert!(!channel.label().is_empty());
            assert!(!channel.target_label().is_empty());
            assert!(
                clean_target(channel, channel.target_placeholder()).is_some(),
                "{channel:?}"
            );
            assert_eq!(
                serde_json::to_string(&channel).unwrap(),
                format!("\"{}\"", channel.as_str())
            );
        }
    }

    #[test]
    fn phone_numbers_are_normalised_to_digits_with_country_code() {
        assert_eq!(clean_phone("+49 170 1234567").as_deref(), Some("491701234567"));
        assert_eq!(clean_phone("0049 (170) 123-4567").as_deref(), Some("491701234567"));
        assert_eq!(clean_phone("0170/1234567").as_deref(), Some("491701234567"));
        assert_eq!(clean_phone("491701234567").as_deref(), Some("491701234567"));
        assert_eq!(clean_phone("+1 415 555 2671").as_deref(), Some("14155552671"));
    }

    #[test]
    fn broken_phone_numbers_are_rejected() {
        for bad in [
            "",
            "12345",
            "+49 170 abc",
            "+49+170123456",
            "1234567890123456",
            "+0 123456789",
            "tel:123456789",
        ] {
            assert_eq!(clean_phone(bad), None, "{bad}");
        }
    }

    #[test]
    fn chat_ids_are_numbers_or_channel_names() {
        assert_eq!(clean_chat_id(" 123456789 ").as_deref(), Some("123456789"));
        assert_eq!(clean_chat_id("-1001234567890").as_deref(), Some("-1001234567890"));
        assert_eq!(clean_chat_id("@bandits_alarm").as_deref(), Some("@bandits_alarm"));
        for bad in ["", "12", "abc", "@ab", "12 34 56", "@mit-strich", "--12345"] {
            assert_eq!(clean_chat_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn email_addresses_are_checked_roughly_and_cannot_inject_headers() {
        assert_eq!(clean_email(" noah@beispiel.de ").as_deref(), Some("noah@beispiel.de"));
        for bad in [
            "",
            "noah",
            "noah@",
            "@beispiel.de",
            "noah@beispiel",
            "noah@@beispiel.de",
            "noah@.de",
            "noah@beispiel.",
            "noah@bei..spiel.de",
            "no ah@beispiel.de",
            "noah@beispiel.de\r\nBcc: x@y.z",
            "a@b.de,c@d.de",
            "<noah@beispiel.de>",
        ] {
            assert_eq!(clean_email(bad), None, "{bad}");
        }
        assert_eq!(clean_email(&format!("{}@beispiel.de", "a".repeat(250))), None);
    }

    #[test]
    fn a_rule_only_sends_what_it_is_asked_for() {
        let r = rule();
        assert!(r.wants(AlertKind::Critical) && r.wants(AlertKind::Offline) && r.wants(AlertKind::Fault));
        assert!(!r.wants(AlertKind::Stagnation) && !r.wants(AlertKind::Drop));
        assert_eq!(r.channels_for(AlertKind::Critical), [Channel::Telegram, Channel::Email]);
        assert!(r.channels_for(AlertKind::Drop).is_empty());

        let everything = NotificationRule {
            only_critical: false,
            ..rule()
        };
        assert_eq!(everything.channels_for(AlertKind::Drop).len(), 2);

        let off = NotificationRule {
            enabled: false,
            ..rule()
        };
        assert!(off.channels_for(AlertKind::Critical).is_empty());
    }

    #[test]
    fn a_channel_without_a_valid_target_is_skipped() {
        let r = NotificationRule {
            sms: true,
            sms_number: "quatsch".into(),
            whatsapp: true,
            ..rule()
        };
        assert_eq!(r.channels_for(AlertKind::Critical), [Channel::Telegram, Channel::Email]);
    }

    #[test]
    fn validation_names_the_channel_and_what_is_wrong() {
        assert_eq!(rule().validate(), Ok(()));
        assert_eq!(
            NotificationRule::default().validate(),
            Ok(()),
            "alles aus und leer ist in Ordnung"
        );
        let missing = NotificationRule { sms: true, ..rule() };
        assert_eq!(
            missing.validate().unwrap_err(),
            "SMS: Handynummer mit Ländervorwahl fehlt."
        );
        let wrong = NotificationRule {
            email_address: "kaputt".into(),
            ..rule()
        };
        assert_eq!(
            wrong.validate().unwrap_err(),
            "E-Mail: E-Mail-Adresse ist nicht gültig."
        );
        // Auch ein ausgeschalteter Dienst darf kein kaputtes Ziel speichern.
        let stale = NotificationRule {
            whatsapp: false,
            whatsapp_number: "123".into(),
            ..rule()
        };
        assert!(stale.validate().is_err());
    }

    #[test]
    fn targets_and_switches_are_reachable_by_channel() {
        let mut r = NotificationRule::default();
        for channel in Channel::ALL {
            assert!(!r.is_on(channel));
            r.set_on(channel, true);
            r.set_target(channel, &format!("  {}  ", channel.target_placeholder()));
            assert!(r.is_on(channel));
            assert_eq!(r.target(channel), channel.target_placeholder());
        }
        let status = ChannelStatus {
            telegram: true,
            ..ChannelStatus::default()
        };
        assert!(status.ready(Channel::Telegram));
        assert!(!status.ready(Channel::WhatsApp) && !status.ready(Channel::Sms) && !status.ready(Channel::Email));
    }

    #[test]
    fn the_alert_text_is_short_and_names_device_and_reason() {
        assert_eq!(
            alert_text("TN0001", Some("Kreuzung Nord"), AlertKind::Critical, Some(12.4)),
            "Battery Bandits: TN0001 (Kreuzung Nord), Ladestand kritisch. Ladestand 12 %."
        );
        assert_eq!(
            alert_text("101", None, AlertKind::Offline, Some(80.0)),
            "Battery Bandits: 101, Keine Messwerte mehr.",
            "bei Funkstille wäre der alte Ladestand irreführend"
        );
        assert_eq!(
            alert_text("101", Some("  "), AlertKind::Fault, None),
            "Battery Bandits: 101, Ausgefallen trotz hohem Ladestand."
        );
        assert!(
            alert_text("TN0001", Some("Kreuzung Nord"), AlertKind::Critical, Some(12.4))
                .chars()
                .count()
                < 160
        );
        assert_eq!(
            alert_subject("TN0001", AlertKind::Drop),
            "Battery Bandits: TN0001, Spannung bricht plötzlich ein"
        );
    }

    #[test]
    fn the_link_to_the_device_is_appended_when_the_address_is_known() {
        assert_eq!(
            with_link("Text.", Some("https://staging.example.org/"), Some("TN0001")),
            "Text. https://staging.example.org/geraete/TN0001"
        );
        assert_eq!(with_link("Text.", Some("https://x.org"), None), "Text. https://x.org");
        assert_eq!(with_link("Text.", None, Some("TN0001")), "Text.");
        assert_eq!(with_link("Text.", Some("  "), Some("TN0001")), "Text.");
    }

    #[test]
    fn share_links_open_the_right_app_with_encoded_text() {
        let text = "TN0001: Ladestand kritisch, 12 %";
        assert_eq!(
            share_url(Channel::WhatsApp, None, "", text, None),
            "https://wa.me/?text=TN0001%3A%20Ladestand%20kritisch%2C%2012%20%25"
        );
        assert_eq!(
            share_url(Channel::WhatsApp, Some("+49 170 1234567"), "", "Hallo", None),
            "https://wa.me/491701234567?text=Hallo"
        );
        assert_eq!(
            share_url(
                Channel::Telegram,
                None,
                "",
                "Hallo Welt",
                Some("https://x.org/geraete/TN1")
            ),
            "https://t.me/share/url?url=https%3A%2F%2Fx.org%2Fgeraete%2FTN1&text=Hallo%20Welt"
        );
        assert_eq!(
            share_url(Channel::Telegram, None, "", "Hallo Welt", None),
            "https://t.me/share/url?url=Hallo%20Welt"
        );
        assert_eq!(
            share_url(Channel::Sms, None, "", "Hallo Welt", None),
            "sms:?&body=Hallo%20Welt"
        );
        assert_eq!(
            share_url(Channel::Sms, Some("0170 1234567"), "", "Hi", Some("https://x.org")),
            "sms:+491701234567?&body=Hi%20https%3A%2F%2Fx.org"
        );
        assert_eq!(
            share_url(Channel::Email, Some("noah@beispiel.de"), "Betreff ä", "Text", None),
            "mailto:noah@beispiel.de?subject=Betreff%20%C3%A4&body=Text"
        );
    }

    #[test]
    fn an_invalid_target_falls_back_to_choosing_the_recipient_in_the_app() {
        assert_eq!(
            share_url(Channel::WhatsApp, Some("quatsch"), "", "Hi", None),
            "https://wa.me/?text=Hi"
        );
        assert_eq!(
            share_url(Channel::Email, Some("kaputt"), "S", "T", None),
            "mailto:?subject=S&body=T"
        );
    }
}
