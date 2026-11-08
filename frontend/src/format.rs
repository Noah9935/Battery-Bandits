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
    match kind {
        AlertKind::Critical => "Ladestand kritisch",
        AlertKind::Offline => "Keine Messwerte mehr",
    }
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
        assert!(texts.iter().all(|t| !t.contains(['–', '—'])));
    }
}
