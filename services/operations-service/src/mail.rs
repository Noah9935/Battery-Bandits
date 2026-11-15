//! Gestaltete E-Mails: Eine Meldung auf einen Blick, kurz und mit dem Logo.
//!
//! Jede Mail besteht aus einem Textteil (für Programme ohne HTML) und einem HTML-Teil. Das HTML nutzt nur Tabellen und
//! Inline-Stile, damit es in Gmail, Outlook, Apple Mail und auf dem Handy gleich aussieht. Das Logo hängt als eingebettetes
//! Bild an der Mail (`cid:logo`), es wird also nichts von einem Server nachgeladen. Alle Angaben aus der Datenbank
//! (Namen, Kennungen) werden maskiert.
//!
//! Aufbau: Kopf mit Logo, Art der Meldung als Schild, eine Zeile, was los ist, der Node, der Ladestand als große Zahl
//! mit Balken, dazu bis zu drei Kennzahlen, ein Satz, was zu tun ist, und die Knöpfe "Node öffnen" und "Route".

use chrono::{DateTime, Utc};
use shared::AlertKind;

/// Das Logo (Waschbär mit Blitz), 128 Pixel: scharf genug für Bildschirme mit doppelter Auflösung bei 44 Pixeln.
pub const LOGO_PNG: &[u8] = include_bytes!("../../../assets/logo/battery-bandits-logo-128.png");
/// Kennung, unter der das Logo in der Mail eingebettet ist (`<img src="cid:logo">`).
pub const LOGO_CID: &str = "logo";
/// Absendername, wenn `SMTP_FROM` nur eine Adresse enthält.
pub const SENDER_NAME: &str = "Battery Bandits";

const AMBER: &str = "#E9A23B";
const INK: &str = "#17171A";
const FONT: &str = "-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif";

/// Eine fertige Mail: Betreff, Text und HTML.
#[derive(Debug, Clone, PartialEq)]
pub struct MailContent {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// Was die Mail über eine Meldung wissen muss.
pub struct AlertFacts<'a> {
    pub device_id: &'a str,
    pub name: Option<&'a str>,
    pub kind: AlertKind,
    pub soc_percent: Option<f32>,
    pub voltage_mv: Option<i32>,
    pub last_seen: Option<DateTime<Utc>>,
    pub forecast_hours: Option<f32>,
    pub position: Option<(f64, f64)>,
    /// Adresse der Oberfläche (`PUBLIC_URL`). Ohne sie gibt es keine Knöpfe zum Öffnen.
    pub base_url: Option<&'a str>,
    pub now: DateTime<Utc>,
}

/// Farbe und Wortlaut je Art der Meldung.
struct Look {
    badge: &'static str,
    accent: &'static str,
    headline: &'static str,
    hint: &'static str,
}

fn look(kind: AlertKind) -> Look {
    match kind {
        AlertKind::Critical => Look {
            badge: "Kritisch",
            accent: "#D92D20",
            headline: "Batterie fast leer",
            hint: "Bitte bald vor Ort die Batterie wechseln.",
        },
        AlertKind::Offline => Look {
            badge: "Offline",
            accent: "#475467",
            headline: "Node meldet sich nicht mehr",
            hint: "Es kommen keine Messwerte mehr an. Bitte vor Ort nachsehen.",
        },
        AlertKind::Fault => Look {
            badge: "Ausfall",
            accent: "#B42318",
            headline: "Ausgefallen, obwohl noch geladen",
            hint: "Die Batterie war noch gut gefüllt. Vermutlich ein Defekt am Node oder am Anschluss.",
        },
        AlertKind::Drop => Look {
            badge: "Hinweis",
            accent: "#B54708",
            headline: "Spannung bricht ein",
            hint: "Die Spannung ist plötzlich stark gefallen. Bitte im Blick behalten.",
        },
        AlertKind::Stagnation => Look {
            badge: "Hinweis",
            accent: "#B54708",
            headline: "Messwerte bleiben gleich",
            hint: "Der Node sendet, aber die Werte ändern sich nicht. Bitte den Sensor prüfen.",
        },
    }
}

/// Alles, woraus Text und HTML entstehen.
struct Layout {
    subject: String,
    preheader: String,
    badge: &'static str,
    accent: &'static str,
    headline: &'static str,
    /// Kennung und Name des Nodes.
    node: Option<(String, Option<String>)>,
    /// Ladestand in Prozent mit der Beschriftung darunter.
    big: Option<(f32, &'static str)>,
    tiles: Vec<(&'static str, String)>,
    hint: &'static str,
    /// Beschriftung, Adresse, Hauptknopf?
    buttons: Vec<(&'static str, String, bool)>,
    footer: &'static str,
}

const FOOTER: &str = "Du bekommst diese Mail, weil die Meldung für dich bestimmt ist. Ausschalten kannst du sie im Postfach unter \"Weiterleiten\".";

// ---------------------------------------------------------------- Öffentliche Mails

/// Die Mail zu einer neuen Meldung.
pub fn render_alert(facts: &AlertFacts) -> MailContent {
    let look = look(facts.kind);
    let name = facts.name.map(str::trim).filter(|n| !n.is_empty());
    let id = facts.device_id;

    let mut tiles = Vec::new();
    if let Some(mv) = facts.voltage_mv {
        tiles.push(("Spannung", format!("{} V", volts(mv))));
    }
    if matches!(facts.kind, AlertKind::Critical | AlertKind::Drop)
        && let Some(hours) = facts.forecast_hours.filter(|h| *h > 0.0)
    {
        tiles.push(("Leer in", left(hours)));
    }
    if let Some(seen) = facts.last_seen {
        tiles.push(("Zuletzt", ago(facts.now, seen)));
    }

    let mut buttons = Vec::new();
    if let Some(base) = clean_base(facts.base_url) {
        buttons.push(("Node öffnen", format!("{base}/geraete/{}", encode(id)), true));
    }
    if let Some((lat, lon)) = facts.position {
        buttons.push((
            "Route",
            format!("https://www.google.com/maps/dir/?api=1&destination={lat:.6},{lon:.6}"),
            false,
        ));
    }

    let soc = facts.soc_percent;
    let short = match soc {
        Some(soc) if matches!(facts.kind, AlertKind::Critical) => {
            format!("{id}, {} ({} %)", look.headline, percent(soc))
        }
        _ => format!("{id}, {}", look.headline),
    };
    let layout = Layout {
        subject: format!("{}: {short}", look.badge),
        preheader: match soc {
            Some(soc) => format!("{id}: {}. Ladestand {} %.", look.headline, percent(soc)),
            None => format!("{id}: {}.", look.headline),
        },
        badge: look.badge,
        accent: look.accent,
        headline: look.headline,
        node: Some((id.to_owned(), name.map(str::to_owned))),
        big: soc.map(|s| {
            (
                s,
                if facts.kind == AlertKind::Offline {
                    "Ladestand bei der letzten Messung"
                } else {
                    "Ladestand"
                },
            )
        }),
        tiles,
        hint: look.hint,
        buttons,
        footer: FOOTER,
    };
    content(&layout)
}

/// Die Probemail: Sie zeigt, dass die Weiterleitung funktioniert.
pub fn render_test(base_url: Option<&str>) -> MailContent {
    let mut buttons = Vec::new();
    if let Some(base) = clean_base(base_url) {
        buttons.push(("Battery Bandits öffnen", base.to_owned(), true));
    }
    content(&Layout {
        subject: "Probe: Die Weiterleitung per E-Mail funktioniert".to_owned(),
        preheader: "Ab jetzt kommen Meldungen zu deinen Nodes an diese Adresse.".to_owned(),
        badge: "Probe",
        accent: "#067647",
        headline: "Das hat geklappt",
        node: None,
        big: None,
        tiles: Vec::new(),
        hint: "Ab jetzt kommen Meldungen zu deinen Nodes an diese Adresse.",
        buttons,
        footer: FOOTER,
    })
}

fn content(layout: &Layout) -> MailContent {
    MailContent {
        subject: layout.subject.clone(),
        text: render_text(layout),
        html: render_html(layout),
    }
}

// ---------------------------------------------------------------- Text

fn render_text(l: &Layout) -> String {
    let mut out = format!("{}: {}\n", l.badge.to_uppercase(), l.headline);
    if let Some((id, name)) = &l.node {
        match name {
            Some(name) => out.push_str(&format!("Node: {id} ({name})\n")),
            None => out.push_str(&format!("Node: {id}\n")),
        }
    }
    if let Some((soc, label)) = l.big {
        out.push_str(&format!("{label}: {} %\n", percent(soc)));
    }
    for (label, value) in &l.tiles {
        out.push_str(&format!("{label}: {value}\n"));
    }
    out.push('\n');
    out.push_str(l.hint);
    out.push('\n');
    if !l.buttons.is_empty() {
        out.push('\n');
        for (label, url, _) in &l.buttons {
            out.push_str(&format!("{label}: {url}\n"));
        }
    }
    out.push_str(&format!("\nBattery Bandits. {}\n", l.footer));
    out
}

// ---------------------------------------------------------------- HTML

/// Hell und dunkel: Apple Mail und Outlook.com folgen dem Farbschema des Geräts.
const STYLE: &str = "<style>\
@media (prefers-color-scheme: dark) {\
 .bg { background: #0B0B0D !important; }\
 .card { background: #17171A !important; border-color: #2A2A2E !important; }\
 .txt { color: #F2F4F7 !important; }\
 .muted { color: #A0A4AE !important; }\
 .tile { background: #1F1F23 !important; }\
 .bar-rest { background: #2E2E33 !important; }\
 .btn2 { color: #F2F4F7 !important; border-color: #3A3A40 !important; }\
 .rule { border-color: #2A2A2E !important; }\
}\
@media (max-width: 480px) { .pad { padding-left: 18px !important; padding-right: 18px !important; } .big { font-size: 48px !important; } }\
</style>";

fn render_html(l: &Layout) -> String {
    let accent = l.accent;
    let mut h = String::with_capacity(8_000);
    h.push_str("<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    h.push_str("<meta name=\"color-scheme\" content=\"light dark\"><meta name=\"supported-color-schemes\" content=\"light dark\">");
    h.push_str(&format!("<title>{}</title>{STYLE}</head>", esc(&l.subject)));
    h.push_str("<body class=\"bg\" style=\"margin:0;padding:0;background:#F2F4F7;\">");
    // Vorschautext in der Postfachliste, im Mailprogramm unsichtbar.
    h.push_str(&format!(
        "<div style=\"display:none;max-height:0;overflow:hidden;opacity:0;color:transparent;\">{}</div>",
        esc(&l.preheader)
    ));
    h.push_str("<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"bg\" style=\"background:#F2F4F7;\"><tr><td align=\"center\" style=\"padding:24px 12px;\">");
    h.push_str(&format!(
        "<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" class=\"card\" style=\"max-width:560px;background:#FFFFFF;border:1px solid #E4E7EC;border-radius:16px;overflow:hidden;font-family:{FONT};\">"
    ));

    // Kopf: Logo auf weißer Fläche, damit der dunkle Waschbär auch im dunklen Modus sichtbar bleibt.
    h.push_str(&format!(
        "<tr><td class=\"pad\" style=\"background:{INK};padding:16px 24px;border-bottom:3px solid {AMBER};\"><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\"><tr>\
<td width=\"52\" valign=\"middle\"><table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\"><tr><td style=\"background:#FFFFFF;border-radius:12px;padding:4px;\"><img src=\"cid:{LOGO_CID}\" width=\"44\" height=\"44\" alt=\"Battery Bandits\" style=\"display:block;border:0;width:44px;height:44px;\"></td></tr></table></td>\
<td valign=\"middle\" style=\"font-family:{FONT};font-size:18px;font-weight:700;color:#FFFFFF;padding-left:12px;\">Battery Bandits</td>\
<td align=\"right\" valign=\"middle\" style=\"font-family:{FONT};font-size:12px;color:#A0A4AE;letter-spacing:.08em;text-transform:uppercase;\">Meldung</td>\
</tr></table></td></tr>"
    ));

    // Schild, Überschrift, Node
    h.push_str("<tr><td class=\"pad\" style=\"padding:28px 24px 4px 24px;\">");
    h.push_str(&format!(
        "<span style=\"display:inline-block;padding:6px 12px;border-radius:999px;background:{accent};color:#FFFFFF;font-family:{FONT};font-size:12px;font-weight:700;line-height:1;letter-spacing:.08em;text-transform:uppercase;\">{}</span>",
        esc(l.badge)
    ));
    h.push_str(&format!(
        "<h1 class=\"txt\" style=\"margin:14px 0 6px 0;font-family:{FONT};font-size:26px;line-height:1.2;font-weight:700;color:#101828;\">{}</h1>",
        esc(l.headline)
    ));
    if let Some((id, name)) = &l.node {
        let name_html = match name {
            Some(name) => format!(" &middot; {}", esc(name)),
            None => String::new(),
        };
        h.push_str(&format!(
            "<p class=\"muted\" style=\"margin:0;font-family:{FONT};font-size:16px;line-height:1.4;color:#475467;\"><strong class=\"txt\" style=\"color:#101828;\">{}</strong>{name_html}</p>",
            esc(id)
        ));
    }
    h.push_str("</td></tr>");

    // Ladestand: große Zahl und Balken
    if let Some((soc, label)) = l.big {
        let pct = soc.clamp(0.0, 100.0);
        let filled = pct.round().max(2.0);
        let mut bar = format!(
            "<td width=\"{filled:.0}%\" style=\"background:{accent};height:10px;line-height:10px;font-size:0;\">&nbsp;</td>"
        );
        if filled < 100.0 {
            bar.push_str("<td class=\"bar-rest\" style=\"background:#E4E7EC;height:10px;line-height:10px;font-size:0;\">&nbsp;</td>");
        }
        h.push_str(&format!(
            "<tr><td class=\"pad\" style=\"padding:20px 24px 4px 24px;\">\
<div class=\"big\" style=\"font-family:{FONT};font-size:56px;line-height:1;font-weight:800;color:{accent};\">{}<span style=\"font-size:26px;font-weight:700;\">&nbsp;%</span></div>\
<div class=\"muted\" style=\"font-family:{FONT};font-size:13px;color:#667085;margin:6px 0 12px 0;\">{}</div>\
<table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\" style=\"border-radius:6px;overflow:hidden;\"><tr>{bar}</tr></table>\
</td></tr>",
            percent(soc),
            esc(label)
        ));
    }

    // Kennzahlen
    if !l.tiles.is_empty() {
        let width = 100 / l.tiles.len();
        h.push_str("<tr><td class=\"pad\" style=\"padding:20px 24px 0 24px;\"><table role=\"presentation\" width=\"100%\" cellpadding=\"0\" cellspacing=\"0\"><tr>");
        let last = l.tiles.len() - 1;
        for (i, (label, value)) in l.tiles.iter().enumerate() {
            let gap = if i == last { "" } else { "padding-right:8px;" };
            h.push_str(&format!(
                "<td width=\"{width}%\" valign=\"top\" style=\"{gap}\"><div class=\"tile\" style=\"background:#F9FAFB;border-radius:10px;padding:10px 12px;\">\
<div class=\"muted\" style=\"font-family:{FONT};font-size:11px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:#667085;\">{}</div>\
<div class=\"txt\" style=\"font-family:{FONT};font-size:16px;font-weight:600;color:#101828;margin-top:3px;\">{}</div></div></td>",
                esc(label),
                esc(value)
            ));
        }
        h.push_str("</tr></table></td></tr>");
    }

    // Was ist zu tun?
    h.push_str(&format!(
        "<tr><td class=\"pad\" style=\"padding:20px 24px 0 24px;\"><p class=\"txt\" style=\"margin:0;font-family:{FONT};font-size:16px;line-height:1.5;color:#344054;\">{}</p></td></tr>",
        esc(l.hint)
    ));

    // Knöpfe
    if !l.buttons.is_empty() {
        h.push_str("<tr><td class=\"pad\" style=\"padding:24px 24px 4px 24px;\"><table role=\"presentation\" cellpadding=\"0\" cellspacing=\"0\"><tr>");
        for (label, url, primary) in &l.buttons {
            if *primary {
                h.push_str(&format!(
                    "<td style=\"background:{AMBER};border-radius:10px;\"><a href=\"{}\" style=\"display:inline-block;padding:13px 24px;font-family:{FONT};font-size:15px;font-weight:700;color:{INK};text-decoration:none;border-radius:10px;\">{}</a></td><td width=\"10\">&nbsp;</td>",
                    esc(url),
                    esc(label)
                ));
            } else {
                h.push_str(&format!(
                    "<td style=\"border:1px solid #D0D5DD;border-radius:10px;\" class=\"btn2\"><a href=\"{}\" class=\"btn2 txt\" style=\"display:inline-block;padding:12px 22px;font-family:{FONT};font-size:15px;font-weight:600;color:#344054;text-decoration:none;border-radius:10px;\">{}</a></td><td width=\"10\">&nbsp;</td>",
                    esc(url),
                    esc(label)
                ));
            }
        }
        h.push_str("</tr></table></td></tr>");
    }

    // Fuß
    h.push_str(&format!(
        "<tr><td class=\"pad\" style=\"padding:28px 24px 24px 24px;\"><div class=\"rule\" style=\"border-top:1px solid #E4E7EC;padding-top:16px;font-family:{FONT};font-size:12px;line-height:1.5;color:#98A2B3;\">{}</div></td></tr>",
        esc(l.footer)
    ));
    h.push_str("</table></td></tr></table></body></html>");
    h
}

// ---------------------------------------------------------------- Hilfen

/// Maskiert Text für HTML, auch für Attributwerte.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Prozentkodierung für einen Pfadteil: nur unverfängliche Zeichen bleiben stehen.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Die Adresse der Oberfläche ohne Schrägstrich am Ende, nur wenn sie mit http oder https beginnt.
fn clean_base(base: Option<&str>) -> Option<&str> {
    base.map(|b| b.trim().trim_end_matches('/'))
        .filter(|b| b.starts_with("http://") || b.starts_with("https://"))
}

fn de(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}

fn percent(soc: f32) -> String {
    de(f64::from(soc.clamp(0.0, 100.0)), 0)
}

fn volts(mv: i32) -> String {
    de(f64::from(mv) / 1000.0, 2)
}

/// "gerade eben", "vor 5 Min.", "vor 3 Std.", "vor 2 Tagen".
pub fn ago(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds().max(0);
    match secs {
        0..=59 => "gerade eben".to_owned(),
        60..=3_599 => format!("vor {} Min.", secs / 60),
        3_600..=86_399 => format!("vor {} Std.", secs / 3_600),
        _ => match secs / 86_400 {
            1 => "vor 1 Tag".to_owned(),
            days => format!("vor {days} Tagen"),
        },
    }
}

/// Restlaufzeit der Prognose: "unter 1 Std.", "etwa 5 Std.", "etwa 3 Tage".
fn left(hours: f32) -> String {
    if hours < 1.0 {
        "unter 1 Std.".to_owned()
    } else if hours < 48.0 {
        format!("etwa {} Std.", hours.round())
    } else {
        format!("etwa {} Tage", (hours / 24.0).round())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap()
    }

    fn facts(kind: AlertKind) -> AlertFacts<'static> {
        AlertFacts {
            device_id: "SIM30",
            name: Some("Killesberg"),
            kind,
            soc_percent: Some(6.7),
            voltage_mv: Some(10_660),
            last_seen: Some(now() - chrono::Duration::minutes(2)),
            forecast_hours: Some(3.2),
            position: Some((48.8, 9.165)),
            base_url: Some("https://staging.example.org/"),
            now: now(),
        }
    }

    #[test]
    fn the_subject_says_what_is_going_on_in_a_few_words() {
        let subject = |kind| render_alert(&facts(kind)).subject;
        assert_eq!(
            subject(AlertKind::Critical),
            "Kritisch: SIM30, Batterie fast leer (7 %)"
        );
        assert_eq!(
            subject(AlertKind::Offline),
            "Offline: SIM30, Node meldet sich nicht mehr"
        );
        assert_eq!(
            subject(AlertKind::Fault),
            "Ausfall: SIM30, Ausgefallen, obwohl noch geladen"
        );
        assert_eq!(subject(AlertKind::Drop), "Hinweis: SIM30, Spannung bricht ein");
        assert_eq!(
            subject(AlertKind::Stagnation),
            "Hinweis: SIM30, Messwerte bleiben gleich"
        );
    }

    #[test]
    fn the_html_shows_the_important_things_at_a_glance() {
        let mail = render_alert(&facts(AlertKind::Critical));
        let html = &mail.html;
        for expected in [
            "Kritisch",
            "Batterie fast leer",
            "SIM30",
            "Killesberg",
            "10,66 V",
            "etwa 3 Std.",
            "vor 2 Min.",
            "Node öffnen",
            "Route",
            "https://staging.example.org/geraete/SIM30",
            "destination=48.800000,9.165000",
            "Ladestand",
        ] {
            assert!(html.contains(expected), "{expected} fehlt");
        }
        // 6,7 Prozent werden als 7 gezeigt, der Balken ist entsprechend lang.
        assert!(html.contains(">7<span"), "große Zahl");
        assert!(html.contains("width=\"7%\""), "Balken");
        assert!(html.contains("#D92D20"), "Farbe für kritisch");
    }

    #[test]
    fn the_logo_is_attached_and_nothing_is_loaded_from_the_web() {
        let html = render_alert(&facts(AlertKind::Critical)).html;
        assert!(html.contains("src=\"cid:logo\""));
        assert!(!html.contains("src=\"http"), "keine Bilder aus dem Netz");
        assert!(!html.contains("<script"));
        assert!(LOGO_PNG.starts_with(&[0x89, b'P', b'N', b'G']), "das Logo ist ein PNG");
    }

    #[test]
    fn names_from_the_database_cannot_inject_html() {
        let mut f = facts(AlertKind::Critical);
        f.device_id = "A<b>1";
        f.name = Some("\"><script>alert(1)</script>");
        let mail = render_alert(&f);
        assert!(!mail.html.contains("<script>alert"));
        assert!(!mail.html.contains("<b>1"));
        assert!(mail.html.contains("&lt;script&gt;"));
        assert!(mail.html.contains("/geraete/A%3Cb%3E1"), "auch im Link kodiert");
    }

    #[test]
    fn the_buttons_depend_on_what_is_known() {
        let mut f = facts(AlertKind::Offline);
        f.base_url = None;
        f.position = None;
        let plain = render_alert(&f);
        assert!(!plain.html.contains("Node öffnen") && !plain.html.contains("Route"));
        assert!(!plain.text.contains("http"));

        f.position = Some((48.8, 9.1));
        let route_only = render_alert(&f);
        assert!(route_only.html.contains("Route") && !route_only.html.contains("Node öffnen"));

        f.base_url = Some("javascript:alert(1)");
        assert!(!render_alert(&f).html.contains("javascript:"), "nur http und https");
    }

    #[test]
    fn missing_data_leaves_out_its_part_instead_of_showing_gaps() {
        let mut f = facts(AlertKind::Offline);
        f.soc_percent = None;
        f.voltage_mv = None;
        f.forecast_hours = None;
        f.last_seen = None;
        let mail = render_alert(&f);
        assert!(!mail.html.contains("Ladestand") && !mail.html.contains("Spannung"));
        assert!(!mail.html.contains("Zuletzt"));
        assert!(mail.html.contains("Node meldet sich nicht mehr"));
        // Offline: die Prognose gehört nicht dazu, auch wenn eine da wäre.
        let mut g = facts(AlertKind::Offline);
        g.forecast_hours = Some(5.0);
        assert!(!render_alert(&g).html.contains("Leer in"));
        assert!(
            render_alert(&facts(AlertKind::Offline))
                .html
                .contains("Ladestand bei der letzten Messung")
        );
    }

    #[test]
    fn the_plain_text_is_short_and_has_no_markup() {
        let mail = render_alert(&facts(AlertKind::Critical));
        assert!(!mail.text.contains('<') && !mail.text.contains('>'));
        assert!(
            mail.text
                .starts_with("KRITISCH: Batterie fast leer\nNode: SIM30 (Killesberg)\n")
        );
        assert!(mail.text.contains("Ladestand: 7 %"));
        assert!(
            mail.text
                .contains("Node öffnen: https://staging.example.org/geraete/SIM30")
        );
        assert!(mail.text.lines().count() < 18, "{}", mail.text);
    }

    #[test]
    fn every_kind_has_its_own_headline_and_hint() {
        let kinds = [
            AlertKind::Critical,
            AlertKind::Offline,
            AlertKind::Fault,
            AlertKind::Drop,
            AlertKind::Stagnation,
        ];
        let headlines: std::collections::BTreeSet<&str> = kinds.iter().map(|k| look(*k).headline).collect();
        assert_eq!(headlines.len(), kinds.len());
        for kind in kinds {
            let l = look(kind);
            assert!(!l.hint.is_empty() && l.accent.starts_with('#'));
            assert!(!l.headline.contains(['–', '—']), "keine Gedankenstriche");
        }
    }

    #[test]
    fn the_bar_handles_the_extremes() {
        let mut f = facts(AlertKind::Critical);
        f.soc_percent = Some(0.0);
        assert!(
            render_alert(&f).html.contains("width=\"2%\""),
            "mindestens ein sichtbarer Rest"
        );
        f.soc_percent = Some(100.0);
        let full = render_alert(&f).html;
        assert!(
            full.contains("width=\"100%\"") && !full.contains("bar-rest\" style"),
            "ganz voll ohne Rest"
        );
        f.soc_percent = Some(250.0);
        assert!(
            render_alert(&f).text.contains("Ladestand: 100 %"),
            "Werte werden begrenzt"
        );
    }

    #[test]
    fn the_test_mail_confirms_and_links_to_the_app() {
        let mail = render_test(Some("https://staging.example.org"));
        assert!(mail.subject.starts_with("Probe:"));
        assert!(mail.html.contains("Das hat geklappt") && mail.html.contains("Battery Bandits öffnen"));
        assert!(mail.html.contains("href=\"https://staging.example.org\""));
        assert!(!render_test(None).html.contains("öffnen"));
    }

    #[test]
    fn times_and_numbers_read_naturally_in_german() {
        let n = now();
        assert_eq!(ago(n, n), "gerade eben");
        assert_eq!(ago(n, n - chrono::Duration::minutes(5)), "vor 5 Min.");
        assert_eq!(ago(n, n - chrono::Duration::hours(3)), "vor 3 Std.");
        assert_eq!(ago(n, n - chrono::Duration::hours(30)), "vor 1 Tag");
        assert_eq!(ago(n, n - chrono::Duration::days(236)), "vor 236 Tagen");
        assert_eq!(
            ago(n, n + chrono::Duration::hours(1)),
            "gerade eben",
            "Zukunft wird nicht negativ"
        );
        assert_eq!(left(0.4), "unter 1 Std.");
        assert_eq!(left(5.2), "etwa 5 Std.");
        assert_eq!(left(72.0), "etwa 3 Tage");
        assert_eq!(volts(10_660), "10,66");
        assert_eq!(percent(12.6), "13");
    }

    /// Schreibt alle Mailarten als HTML-Dateien zum Ansehen im Browser, das Logo daneben als `logo.png`:
    /// `MAIL_PREVIEW_DIR=/work/dev/mail-preview cargo test -p operations-service write_previews -- --ignored`
    #[test]
    #[ignore = "schreibt Dateien, nur zum Ansehen der Mails"]
    fn write_previews() {
        let Ok(dir) = std::env::var("MAIL_PREVIEW_DIR") else {
            return;
        };
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}/logo.png"), LOGO_PNG).unwrap();
        let mut all = vec![("probe", render_test(Some("https://staging.example.org")))];
        for (name, kind) in [
            ("kritisch", AlertKind::Critical),
            ("offline", AlertKind::Offline),
            ("ausfall", AlertKind::Fault),
            ("spannung", AlertKind::Drop),
            ("stillstand", AlertKind::Stagnation),
        ] {
            all.push((name, render_alert(&facts(kind))));
        }
        for (name, mail) in all {
            std::fs::write(format!("{dir}/{name}.html"), mail.html.replace("cid:logo", "logo.png")).unwrap();
            std::fs::write(format!("{dir}/{name}.txt"), mail.text).unwrap();
        }
    }
}
