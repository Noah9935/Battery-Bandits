//! Formatierung für die Anzeige. Alles deutsch (Dezimalkomma).

/// `87,5 %`
pub fn percent(value: f64) -> String {
    format!("{} %", number(value, 1))
}

/// Zahl mit Dezimalkomma und fester Nachkommastelle.
pub fn number(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}").replace('.', ",")
}

/// Wie lange etwas her ist: `gerade eben`, `vor 5 Min.`, `vor 3 Std.`, `vor 2 Tagen`.
pub fn age(delta_ms: i64) -> String {
    let secs = delta_ms.max(0) / 1000;
    match secs {
        0..=44 => "gerade eben".to_owned(),
        45..=3599 => format!("vor {} Min.", (secs / 60).max(1)),
        3600..=86_399 => format!("vor {} Std.", secs / 3600),
        _ => {
            let days = secs / 86_400;
            if days == 1 {
                "vor 1 Tag".to_owned()
            } else {
                format!("vor {days} Tagen")
            }
        }
    }
}

/// Anzeigename eines Meldungsstatus.
pub fn incident_state(state: shared::api::IncidentState) -> &'static str {
    use shared::api::IncidentState::*;
    match state {
        Open => "Offen",
        Dismissed => "Abgewiesen",
        Resolved => "Erledigt",
    }
}

pub fn status(status: shared::Status) -> &'static str {
    match status {
        shared::Status::Critical => "Kritisch",
        _ => "In Ordnung",
    }
}

pub fn range(range: shared::api::Range) -> &'static str {
    use shared::api::Range::*;
    match range {
        Day => "24 Stunden",
        Week => "7 Tage",
        Month => "30 Tage",
        Year => "12 Monate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_percent_with_comma() {
        assert_eq!(percent(87.5), "87,5 %");
        assert_eq!(percent(100.0), "100,0 %");
        assert_eq!(percent(0.04), "0,0 %");
    }

    #[test]
    fn formats_numbers() {
        assert_eq!(number(7.456, 2), "7,46");
        assert_eq!(number(5.0, 0), "5");
    }

    #[test]
    fn formats_age() {
        assert_eq!(age(0), "gerade eben");
        assert_eq!(age(44_000), "gerade eben");
        assert_eq!(age(45_000), "vor 1 Min.");
        assert_eq!(age(5 * 60_000), "vor 5 Min.");
        assert_eq!(age(59 * 60_000 + 59_000), "vor 59 Min.");
        assert_eq!(age(3 * 3_600_000), "vor 3 Std.");
        assert_eq!(age(24 * 3_600_000), "vor 1 Tag");
        assert_eq!(age(3 * 24 * 3_600_000), "vor 3 Tagen");
    }

    #[test]
    fn negative_age_counts_as_now() {
        assert_eq!(age(-5_000), "gerade eben");
    }

    #[test]
    fn labels() {
        assert_eq!(status(shared::Status::Critical), "Kritisch");
        assert_eq!(status(shared::Status::Ok), "In Ordnung");
        assert_eq!(incident_state(shared::api::IncidentState::Open), "Offen");
        assert_eq!(range(shared::api::Range::Year), "12 Monate");
    }
}
