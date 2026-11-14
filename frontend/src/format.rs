//! Formatierung für die Anzeige. Alles deutsch (Dezimalkomma), rein und testbar.

use shared::{AlertKind, DeviceStatus};

/// Zahl mit Dezimalkomma und fester Nachkommastellenzahl.
pub fn number(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}

/// `87,5 %`
pub fn percent(value: f32) -> String {
    format!("{} %", number(f64::from(value), 1))
}

/// Spannung in Volt: `12,30 V`
pub fn voltage(mv: i32) -> String {
    format!("{} V", number(f64::from(mv) / 1000.0, 2))
}

/// Entladestrom in Milliampere. Es zählt der Betrag: Das Vorzeichen ist je nach Messgerät unterschiedlich
/// (live negativ, Vorlesungsdaten positiv) und würde nur verwirren.
pub fn current(ma: i32) -> String {
    format!("{} mA", ma.unsigned_abs())
}

/// Strom in Ampere mit einer Nachkommastelle: `25,3 A`. Kleiner Zusatz unter der Spannung.
pub fn amps(ma: i32) -> String {
    format!("{} A", number(f64::from(ma.unsigned_abs()) / 1000.0, 1))
}

pub fn temperature(celsius: f32) -> String {
    format!("{} °C", number(f64::from(celsius), 1))
}

/// Koordinate mit fünf Nachkommastellen.
pub fn coordinate(value: f64) -> String {
    number(value, 5)
}

/// Wie lange etwas her ist: `gerade eben`, `vor 5 Min.`, `vor 3 Std.`, `vor 2 Tagen`.
pub fn age(delta_ms: i64) -> String {
    let secs = delta_ms.max(0) / 1000;
    match secs {
        0..=44 => "gerade eben".to_owned(),
        45..=3599 => format!("vor {} Min.", (secs / 60).max(1)),
        3600..=86_399 => format!("vor {} Std.", secs / 3600),
        _ => match secs / 86_400 {
            1 => "vor 1 Tag".to_owned(),
            days => format!("vor {days} Tagen"),
        },
    }
}

/// Wie lange ein Zustand schon besteht: `unter 1 Min.`, `47 Min.`, `2 Std. 14 Min.`, `3 Tage 4 Std.`.
/// Steht hinter "seit", deshalb im Dativ ("seit 3 Tagen").
pub fn span(delta_ms: i64) -> String {
    let minutes = delta_ms.max(0) / 60_000;
    let (days, hours, mins) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    match (days, hours) {
        (0, 0) if mins == 0 => "unter 1 Min.".to_owned(),
        (0, 0) => format!("{mins} Min."),
        (0, _) => format!("{hours} Std. {mins} Min."),
        (1, _) => format!("1 Tag {hours} Std."),
        _ => format!("{days} Tagen {hours} Std."),
    }
}

/// Laufzeit in Tagen oder Stunden für Tabellen: `36 Std.`, `12,5 Tage`.
pub fn runtime(hours: f64) -> String {
    if hours < 72.0 {
        format!("{} Std.", number(hours, 0))
    } else {
        format!("{} Tage", number(hours / 24.0, 1))
    }
}

/// Restlaufzeit in Worten: `weniger als 1 Std.`, `ca. 28 Std.`, `ca. 3 Tage`.
pub fn duration_hours(hours: f32) -> String {
    if hours < 1.0 {
        "weniger als 1 Std.".to_owned()
    } else if hours < 48.0 {
        format!("ca. {} Std.", hours.round() as i64)
    } else {
        format!("ca. {} Tage", (hours / 24.0).round() as i64)
    }
}

pub fn status_label(status: DeviceStatus) -> &'static str {
    match status {
        DeviceStatus::Normal => "In Ordnung",
        DeviceStatus::Warning => "Warnung",
        DeviceStatus::Critical => "Kritisch",
        DeviceStatus::Offline => "Offline",
    }
}

/// CSS-Klasse der Statusfarbe.
pub fn status_class(status: DeviceStatus) -> &'static str {
    status.as_str()
}

/// Herkunft der Daten: vom Dozenten (echt) oder erfunden (Simulator).
pub fn source_label(source: shared::DataSource) -> &'static str {
    match source {
        shared::DataSource::Dozent => "Dozent",
        shared::DataSource::Simulation => "Erfunden",
    }
}

/// Satz zur Herkunft der Geräte, zum Beispiel `3 vom Dozenten, 12 erfunden.`
pub fn source_summary(real: usize, invented: usize) -> String {
    match (real, invented) {
        (0, 0) => "Noch keine Geräte.".to_owned(),
        (real, 0) => format!("Alle {real} vom Dozenten."),
        (0, invented) => format!("Alle {invented} erfunden."),
        (real, invented) => format!("{real} vom Dozenten, {invented} erfunden."),
    }
}

pub fn alert_kind_label(kind: AlertKind) -> &'static str {
    kind.label()
}

/// Anzeigename eines Geräts: `Hauptbahnhof (TN0001)` oder nur `TN0001`.
pub fn device_title(id: &str, name: Option<&str>) -> String {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("{name} ({id})"),
        None => id.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_say_how_long_a_state_has_lasted() {
        let min = 60_000;
        assert_eq!(span(0), "unter 1 Min.");
        assert_eq!(span(59_999), "unter 1 Min.");
        assert_eq!(
            span(-5 * min),
            "unter 1 Min.",
            "eine Uhr, die vorgeht, ergibt keine negative Dauer"
        );
        assert_eq!(span(47 * min), "47 Min.");
        assert_eq!(span(60 * min), "1 Std. 0 Min.");
        assert_eq!(span(134 * min), "2 Std. 14 Min.");
        assert_eq!(span(24 * 60 * min + 5 * 60 * min), "1 Tag 5 Std.");
        assert_eq!(span(3 * 24 * 60 * min + 4 * 60 * min + 30 * min), "3 Tagen 4 Std.");
    }

    #[test]
    fn runtimes_switch_from_hours_to_days() {
        assert_eq!(runtime(36.4), "36 Std.");
        assert_eq!(runtime(71.9), "72 Std.");
        assert_eq!(runtime(300.0), "12,5 Tage");
    }

    #[test]
    fn numbers_use_decimal_comma() {
        assert_eq!(number(7.456, 2), "7,46");
        assert_eq!(number(5.0, 0), "5");
        assert_eq!(percent(87.5), "87,5 %");
        assert_eq!(percent(100.0), "100,0 %");
    }

    #[test]
    fn measurements_have_units() {
        assert_eq!(voltage(12_296), "12,30 V");
        assert_eq!(current(-311), "311 mA");
        assert_eq!(current(311), "311 mA");
        assert_eq!(temperature(16.44), "16,4 °C");
        assert_eq!(coordinate(48.784), "48,78400");
    }

    #[test]
    fn age_steps() {
        assert_eq!(age(0), "gerade eben");
        assert_eq!(age(44_000), "gerade eben");
        assert_eq!(age(45_000), "vor 1 Min.");
        assert_eq!(age(5 * 60_000), "vor 5 Min.");
        assert_eq!(age(59 * 60_000 + 59_000), "vor 59 Min.");
        assert_eq!(age(3 * 3_600_000), "vor 3 Std.");
        assert_eq!(age(24 * 3_600_000), "vor 1 Tag");
        assert_eq!(age(3 * 24 * 3_600_000), "vor 3 Tagen");
        assert_eq!(age(-5_000), "gerade eben", "Uhrenabweichung zählt als jetzt");
    }

    #[test]
    fn remaining_time_in_words() {
        assert_eq!(duration_hours(0.4), "weniger als 1 Std.");
        assert_eq!(duration_hours(28.2), "ca. 28 Std.");
        assert_eq!(duration_hours(47.4), "ca. 47 Std.");
        assert_eq!(duration_hours(72.0), "ca. 3 Tage");
    }

    #[test]
    fn labels() {
        assert_eq!(status_label(DeviceStatus::Critical), "Kritisch");
        assert_eq!(status_label(DeviceStatus::Normal), "In Ordnung");
        assert_eq!(status_class(DeviceStatus::Offline), "offline");
        assert_eq!(alert_kind_label(AlertKind::Offline), "Keine Messwerte mehr");
        assert_eq!(device_title("TN0001", Some("Hauptbahnhof")), "Hauptbahnhof (TN0001)");
        assert_eq!(device_title("TN0001", Some("  ")), "TN0001");
        assert_eq!(device_title("TN0001", None), "TN0001");
    }

    #[test]
    fn source_labels_distinguish_real_from_invented() {
        assert_eq!(source_label(shared::DataSource::Dozent), "Dozent");
        assert_eq!(source_label(shared::DataSource::Simulation), "Erfunden");
    }

    #[test]
    fn source_summary_reads_naturally() {
        assert_eq!(source_summary(0, 0), "Noch keine Geräte.");
        assert_eq!(source_summary(3, 0), "Alle 3 vom Dozenten.");
        assert_eq!(source_summary(0, 12), "Alle 12 erfunden.");
        assert_eq!(source_summary(3, 12), "3 vom Dozenten, 12 erfunden.");
    }

    #[test]
    fn no_dashes_in_user_visible_text() {
        let texts = [
            status_label(DeviceStatus::Normal),
            status_label(DeviceStatus::Warning),
            status_label(DeviceStatus::Critical),
            status_label(DeviceStatus::Offline),
            alert_kind_label(AlertKind::Critical),
            alert_kind_label(AlertKind::Offline),
        ];
        assert!(texts.iter().all(|t| !t.contains(['\u{2013}', '\u{2014}'])));
    }

    #[test]
    fn amps_are_shown_with_one_decimal_and_without_sign() {
        assert_eq!(amps(25_308), "25,3 A");
        assert_eq!(amps(-25_308), "25,3 A");
        assert_eq!(amps(175), "0,2 A");
        assert_eq!(amps(0), "0,0 A");
    }

    /// Regel für alle Texte der Oberfläche: keine Gedankenstriche und keine Emojis, in keiner Quelldatei.
    #[test]
    fn no_source_file_contains_dashes_or_emojis() {
        let files: [(&str, &str); 43] = [
            ("api.rs", include_str!("api.rs")),
            ("app.rs", include_str!("app.rs")),
            ("chart.rs", include_str!("chart.rs")),
            ("device.rs", include_str!("device.rs")),
            ("features.rs", include_str!("features.rs")),
            ("filter.rs", include_str!("filter.rs")),
            ("format.rs", include_str!("format.rs")),
            ("history.rs", include_str!("history.rs")),
            ("icons.rs", include_str!("icons.rs")),
            ("inbox.rs", include_str!("inbox.rs")),
            ("inputs.rs", include_str!("inputs.rs")),
            ("insights.rs", include_str!("insights.rs")),
            ("jsutil.rs", include_str!("jsutil.rs")),
            ("login.rs", include_str!("login.rs")),
            ("main.rs", include_str!("main.rs")),
            ("maintenance.rs", include_str!("maintenance.rs")),
            ("map_view.rs", include_str!("map_view.rs")),
            ("markers.rs", include_str!("markers.rs")),
            ("overlays.rs", include_str!("overlays.rs")),
            ("overview.rs", include_str!("overview.rs")),
            ("route.rs", include_str!("route.rs")),
            ("search.rs", include_str!("search.rs")),
            ("share.rs", include_str!("share.rs")),
            ("sse.rs", include_str!("sse.rs")),
            ("state.rs", include_str!("state.rs")),
            ("tickets.rs", include_str!("tickets.rs")),
            ("tour.rs", include_str!("tour.rs")),
            ("efficiency_view.rs", include_str!("efficiency_view.rs")),
            ("forward.rs", include_str!("forward.rs")),
            ("gesture.rs", include_str!("gesture.rs")),
            ("pins.rs", include_str!("pins.rs")),
            ("routing.rs", include_str!("routing.rs")),
            ("swap.rs", include_str!("swap.rs")),
            ("swipe.rs", include_str!("swipe.rs")),
            ("urgency.rs", include_str!("urgency.rs")),
            ("limit.rs", include_str!("limit.rs")),
            ("nodes.rs", include_str!("nodes.rs")),
            ("admin.rs", include_str!("admin.rs")),
            ("avatar.rs", include_str!("avatar.rs")),
            ("profile.rs", include_str!("profile.rs")),
            ("style.css", include_str!("../style.css")),
            ("index.html", include_str!("../index.html")),
            ("map.js", include_str!("../map.js")),
        ];
        for (name, text) in files {
            for (number, line) in text.lines().enumerate() {
                for c in line.chars() {
                    let code = c as u32;
                    let dash = code == 0x2013 || code == 0x2014;
                    let emoji = (0x1F300..=0x1FAFF).contains(&code) || (0x2600..=0x27BF).contains(&code);
                    assert!(!dash && !emoji, "{name}:{}: unerlaubtes Zeichen {c:?}", number + 1);
                }
            }
        }
    }
}
