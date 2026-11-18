//! Eigener Zeitraum des Verlaufs: aus den beiden Datumseingaben werden die Grenzen für die Abfrage.
//!
//! Beide Tage zählen mit: "Von 01.11." und "Bis 03.11." heißt drei volle Tage, der Zeitraum endet also am Ende des
//! dritten Tages. Die Tage sind Ortszeit; wie spät Mitternacht in UTC ist, liefert der Aufrufer (`midnight`).

use chrono::{DateTime, Days, NaiveDate, Utc};
use shared::api::{Bounds, Range};

/// Die Grenzen des Zeitraums, nur bei einem eigenen Zeitraum (`Range::Custom`); sonst `Ok(None)`.
/// Fehlertext für die Anzeige, wenn die Eingaben nicht reichen.
pub fn bounds_for(
    range: Range,
    from: &str,
    to: &str,
    midnight: impl Fn(NaiveDate) -> Option<DateTime<Utc>>,
) -> Result<Option<Bounds>, &'static str> {
    if range != Range::Custom {
        return Ok(None);
    }
    let (Some(from), Some(to)) = (parse_day(from), parse_day(to)) else {
        return Err("Bitte Anfang und Ende des Zeitraums wählen.");
    };
    if to < from {
        return Err("Das Ende darf nicht vor dem Anfang liegen.");
    }
    let end = to
        .checked_add_days(Days::new(1))
        .ok_or("Das Datum liegt zu weit in der Zukunft.")?;
    let (Some(start), Some(end)) = (midnight(from), midnight(end)) else {
        return Err("Dieses Datum kann nicht verwendet werden.");
    };
    Bounds::new(start, end)
        .map(Some)
        .ok_or("Das Ende darf nicht vor dem Anfang liegen.")
}

/// Das Datum eines `<input type="date">`: `2026-11-18`. Leer oder unvollständig ergibt `None`.
pub fn parse_day(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

/// Das Datum im Format des Eingabefelds.
pub fn day_text(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// Vorbelegung des eigenen Zeitraums: die letzten sieben Tage samt heute.
pub fn default_days(today: NaiveDate) -> (String, String) {
    let from = today.checked_sub_days(Days::new(6)).unwrap_or(today);
    (day_text(from), day_text(today))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    /// Ortszeit zwei Stunden vor UTC (wie im Sommer in Deutschland): Mitternacht ist 22 Uhr UTC des Vortags.
    fn midnight(day: NaiveDate) -> Option<DateTime<Utc>> {
        Some(Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0)?) - Duration::hours(2))
    }

    fn day(d: u32) -> String {
        format!("2026-11-{d:02}")
    }

    #[test]
    fn only_a_custom_range_has_bounds() {
        for range in [Range::Day, Range::Week, Range::Month, Range::All] {
            assert_eq!(bounds_for(range, "", "", midnight), Ok(None));
        }
    }

    #[test]
    fn both_days_count_so_the_end_is_the_end_of_the_last_day() {
        let bounds = bounds_for(Range::Custom, &day(1), &day(3), midnight).unwrap().unwrap();
        assert_eq!(bounds.seconds(), 3 * 24 * 3600);
        assert_eq!(bounds.from, Utc.with_ymd_and_hms(2026, 10, 31, 22, 0, 0).unwrap());
        assert_eq!(bounds.to, Utc.with_ymd_and_hms(2026, 11, 3, 22, 0, 0).unwrap());
    }

    #[test]
    fn a_single_day_is_allowed() {
        let bounds = bounds_for(Range::Custom, &day(5), &day(5), midnight).unwrap().unwrap();
        assert_eq!(bounds.seconds(), 24 * 3600);
    }

    #[test]
    fn a_missing_or_incomplete_day_is_asked_for() {
        let text = "Bitte Anfang und Ende des Zeitraums wählen.";
        assert_eq!(bounds_for(Range::Custom, "", &day(3), midnight), Err(text));
        assert_eq!(bounds_for(Range::Custom, &day(3), "", midnight), Err(text));
        assert_eq!(bounds_for(Range::Custom, "2026-11", &day(3), midnight), Err(text));
        assert_eq!(bounds_for(Range::Custom, "gestern", "heute", midnight), Err(text));
    }

    #[test]
    fn an_end_before_the_start_is_refused() {
        assert_eq!(
            bounds_for(Range::Custom, &day(9), &day(3), midnight),
            Err("Das Ende darf nicht vor dem Anfang liegen.")
        );
    }

    #[test]
    fn a_day_without_midnight_is_refused_instead_of_panicking() {
        assert!(bounds_for(Range::Custom, &day(1), &day(2), |_| None).is_err());
    }

    #[test]
    fn the_default_is_the_last_seven_days_including_today() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(default_days(today), ("2026-11-12".to_owned(), "2026-11-18".to_owned()));
        let bounds = bounds_for(Range::Custom, &default_days(today).0, &default_days(today).1, midnight)
            .unwrap()
            .unwrap();
        assert_eq!(bounds.seconds(), 7 * 24 * 3600);
    }

    #[test]
    fn parsing_and_printing_a_day_agree() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        assert_eq!(day_text(today), "2026-01-05");
        assert_eq!(parse_day(" 2026-01-05 "), Some(today));
        assert_eq!(parse_day(""), None);
    }
}
