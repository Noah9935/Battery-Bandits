//! Weiterleitung von Meldungen per E-Mail: Regel je Person und der Link für das Mailprogramm.
//!
//! Zwei Wege:
//! * Von Hand: Ein Link öffnet das Mailprogramm mit vorbereitetem Text ([`mailto_url`]). Das braucht keine
//!   Einrichtung auf dem Server.
//! * Automatisch: Der Meldungs- und Einsatz-Service sendet neue Meldungen nach der Regel der Person
//!   ([`NotificationRule`]). Dafür muss der Zugang zum Mailserver auf dem Server hinterlegt sein.

use serde::{Deserialize, Serialize};

use crate::api::encode_header;
use crate::models::AlertKind;

/// Ob der Server E-Mails senden kann (der Zugang zum Mailserver ist in seiner Umgebung hinterlegt).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailStatus {
    pub ready: bool,
}

/// Regel einer Person: Welche Meldungen gehen automatisch an welche E-Mail-Adresse?
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct NotificationRule {
    /// Hauptschalter. Ist er aus, wird nichts gesendet.
    pub enabled: bool,
    /// Nur dringende Meldungen (kritisch, offline, ausgefallen), keine Auffälligkeiten.
    pub only_critical: bool,
    pub email_address: String,
}

impl NotificationRule {
    /// Soll eine Meldung dieser Art überhaupt gesendet werden?
    pub fn wants(&self, kind: AlertKind) -> bool {
        self.enabled && (!self.only_critical || kind.is_urgent())
    }

    /// Geht eine Meldung dieser Art tatsächlich raus: gewünscht und mit gültiger Adresse.
    pub fn sends(&self, kind: AlertKind) -> bool {
        self.wants(kind) && clean_email(&self.email_address).is_some()
    }

    /// Prüft die Eingaben. Eingeschaltet braucht die Regel eine gültige Adresse, ausgeschaltet darf sie leer bleiben,
    /// eine ausgefüllte Adresse muss aber stimmen.
    pub fn validate(&self) -> Result<(), String> {
        let address = self.email_address.trim();
        if address.is_empty() {
            return if self.enabled {
                Err("Die E-Mail-Adresse fehlt.".to_owned())
            } else {
                Ok(())
            };
        }
        if clean_email(address).is_none() {
            return Err("Die E-Mail-Adresse ist nicht gültig.".to_owned());
        }
        Ok(())
    }
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

/// Link, der das Mailprogramm mit vorbereitetem Betreff und Text öffnet. `to` ist die hinterlegte Adresse. Fehlt sie
/// oder ist sie ungültig, wählt man den Empfänger im Mailprogramm selbst.
pub fn mailto_url(to: Option<&str>, subject: &str, body: &str) -> String {
    let to = to.and_then(clean_email).unwrap_or_default();
    format!(
        "mailto:{to}?subject={}&body={}",
        encode_header(subject),
        encode_header(body)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> NotificationRule {
        NotificationRule {
            enabled: true,
            only_critical: true,
            email_address: "noah@beispiel.de".into(),
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
        assert!(r.sends(AlertKind::Critical));
        assert!(!r.sends(AlertKind::Drop));

        let everything = NotificationRule {
            only_critical: false,
            ..rule()
        };
        assert!(everything.sends(AlertKind::Drop) && everything.sends(AlertKind::Stagnation));

        let off = NotificationRule {
            enabled: false,
            ..rule()
        };
        assert!(!off.wants(AlertKind::Critical) && !off.sends(AlertKind::Critical));
    }

    #[test]
    fn without_a_valid_address_nothing_is_sent() {
        for address in ["", "   ", "quatsch"] {
            let r = NotificationRule {
                email_address: address.into(),
                ..rule()
            };
            assert!(r.wants(AlertKind::Critical), "{address:?}");
            assert!(!r.sends(AlertKind::Critical), "{address:?}");
        }
    }

    #[test]
    fn validation_says_what_is_wrong() {
        assert_eq!(rule().validate(), Ok(()));
        assert_eq!(
            NotificationRule::default().validate(),
            Ok(()),
            "alles aus und leer ist in Ordnung"
        );
        let missing = NotificationRule {
            email_address: "  ".into(),
            ..rule()
        };
        assert_eq!(missing.validate().unwrap_err(), "Die E-Mail-Adresse fehlt.");
        let wrong = NotificationRule {
            email_address: "kaputt".into(),
            ..rule()
        };
        assert_eq!(wrong.validate().unwrap_err(), "Die E-Mail-Adresse ist nicht gültig.");
        // Auch eine ausgeschaltete Regel darf keine kaputte Adresse speichern.
        let stale = NotificationRule {
            enabled: false,
            ..wrong
        };
        assert!(stale.validate().is_err());
    }

    #[test]
    fn the_mail_link_carries_subject_and_text_encoded() {
        assert_eq!(
            mailto_url(Some("noah@beispiel.de"), "Betreff ä", "Text"),
            "mailto:noah@beispiel.de?subject=Betreff%20%C3%A4&body=Text"
        );
        assert_eq!(
            mailto_url(None, "TN0001: Ladestand kritisch", "12 %"),
            "mailto:?subject=TN0001%3A%20Ladestand%20kritisch&body=12%20%25"
        );
    }

    #[test]
    fn an_invalid_address_falls_back_to_choosing_the_recipient_in_the_mail_program() {
        assert_eq!(mailto_url(Some("kaputt"), "S", "T"), "mailto:?subject=S&body=T");
    }
}
