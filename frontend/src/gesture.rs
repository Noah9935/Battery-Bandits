//! Wischgesten auf dem Handy (rein, ohne Browser): Wann ist eine Fingerbewegung ein Seitenwechsel, wann wischt
//! sie einen Hinweis weg?

/// Richtung eines erkannten Wischens. `Left` heißt: Der Finger ging nach links, die nächste Seite kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swipe {
    Left,
    Right,
}

/// Mindestweg in Pixeln, damit ein Wischen zählt. Kürzere Bewegungen sind Tippen oder Zittern.
pub const MIN_DISTANCE: f64 = 56.0;
/// Mindesttempo in Pixeln pro Millisekunde für ein schnelles ("heftiges") Wischen.
pub const MIN_VELOCITY: f64 = 0.3;
/// Wie viel waagerechter die Bewegung sein muss als senkrecht. Darunter ist es Scrollen.
pub const MIN_SLOPE: f64 = 1.7;
/// Ein langsames Wischen zählt, wenn es über diesen Anteil der Bildschirmbreite geht.
pub const SLOW_SHARE: f64 = 0.4;

/// Erkennt einen Seitenwechsel aus dem Weg des Fingers (`dx`, `dy` in Pixeln), der Dauer in Millisekunden und
/// der Breite des Bildschirms. Senkrechtes Scrollen und kurze Bewegungen ergeben `None`.
pub fn page_swipe(dx: f64, dy: f64, ms: f64, width: f64) -> Option<Swipe> {
    let (ax, ay) = (dx.abs(), dy.abs());
    if !(ax.is_finite() && ay.is_finite()) || ax < MIN_DISTANCE || ax < ay * MIN_SLOPE {
        return None;
    }
    let velocity = ax / ms.max(1.0);
    let far = width > 0.0 && ax >= width * SLOW_SHARE;
    (velocity >= MIN_VELOCITY || far).then_some(if dx < 0.0 { Swipe::Left } else { Swipe::Right })
}

/// Nummer der Nachbarseite nach einem Wischen, `None` am Rand der Reihe.
pub fn neighbor(count: usize, current: usize, swipe: Swipe) -> Option<usize> {
    match swipe {
        Swipe::Left => (current + 1 < count).then_some(current + 1),
        Swipe::Right => current.checked_sub(1).filter(|i| *i < count),
    }
}

/// Abstand zwischen zwei Karten der Leiste in Pixeln (`gap` im Stylesheet). Eine Seite der Leiste ist so breit wie die
/// Leiste plus dieser Abstand. Wer ihn vergisst, landet bei jeder Karte ein Stück weiter daneben, und das Einrasten
/// zieht die Leiste dann ruckelnd zurück.
pub const CARD_GAP: f64 = 12.0;

/// Wohin die waagerechte Leiste (Ticker) als Nächstes scrollt. Am Ende geht es zurück zum Anfang.
/// `None`, wenn nichts zu scrollen ist (eine Karte oder unsichtbar).
pub fn next_card_offset(left: f64, width: f64, total: f64) -> Option<f64> {
    if !(left.is_finite() && width.is_finite() && total.is_finite()) || width <= 0.0 || total <= width + 1.0 {
        return None;
    }
    let (last, stride) = (total - width, width + CARD_GAP);
    if left + 1.0 >= last {
        return Some(0.0);
    }
    Some((((left / stride).round() + 1.0) * stride).min(last))
}

/// Wohin die Leiste beim Pfeil nach links scrollt. Am Anfang geht es ans Ende.
pub fn prev_card_offset(left: f64, width: f64, total: f64) -> Option<f64> {
    if !(left.is_finite() && width.is_finite() && total.is_finite()) || width <= 0.0 || total <= width + 1.0 {
        return None;
    }
    let (last, stride) = (total - width, width + CARD_GAP);
    if left <= 1.0 {
        return Some(last);
    }
    Some((((left / stride).round() - 1.0) * stride).max(0.0))
}

/// Wohin die Leiste für die Seite `index` (ab 0) scrollt, nie über das Ende hinaus.
pub fn page_offset(index: usize, width: f64, total: f64) -> f64 {
    if !(width.is_finite() && total.is_finite()) || width <= 0.0 {
        return 0.0;
    }
    (index as f64 * (width + CARD_GAP)).min((total - width).max(0.0))
}

/// Alle zehn Sekunden kommt die nächste Karte.
pub const TICKER_INTERVAL_MS: u32 = 10_000;
/// So lange gleitet die Leiste von selbst (langsam und ruhig).
pub const TICKER_SLIDE_MS: f64 = 1_100.0;
/// So lange gleitet sie nach einem Tipp auf Pfeil oder Punkt.
pub const ARROW_SLIDE_MS: f64 = 650.0;

/// Nummer der Karte, die gerade in der Leiste steht.
pub fn card_index(left: f64, width: f64) -> usize {
    if width <= 0.0 || !left.is_finite() || left < 0.0 {
        return 0;
    }
    (left / (width + CARD_GAP)).round() as usize
}

/// So lange nach der letzten Berührung bleibt das Durchlaufen aus, damit es nicht in das Nachgleiten des Fingers greift.
pub const TOUCH_PAUSE_MS: f64 = 6_000.0;

/// So viele Punkte zeigt die Leiste unter den Karten höchstens. Bei mehr Karten wandert ein Fenster mit.
pub const MAX_DOTS: usize = 15;

/// Welche Karten als Punkte erscheinen: höchstens [`MAX_DOTS`], in festen Seiten. Die Punkte bleiben stehen, der aktive
/// wandert nach rechts, und erst nach dem letzten Punkt der Seite kommen die nächsten (die letzte Seite kann kürzer sein).
pub fn dot_window(current: usize, total: usize) -> std::ops::Range<usize> {
    let start = (current.min(total.saturating_sub(1)) / MAX_DOTS) * MAX_DOTS;
    start..(start + MAX_DOTS).min(total)
}

/// Zusätzliche Höhe des Kartenblatts über seine Standardgröße hinaus, nachdem man den Griff um `dy` Pixel gezogen hat
/// (nach oben ist `dy` negativ). Die Standardgröße ist 0, nach oben wächst das Blatt höchstens um `max_extra`. Eine
/// kleine Restgröße rastet auf die Standardgröße ein.
pub fn sheet_extra_after_drag(start_extra: f64, dy: f64, max_extra: f64) -> f64 {
    let extra = (start_extra - dy).clamp(0.0, max_extra.max(0.0));
    if extra < 24.0 { 0.0 } else { extra }
}

/// Zieht dieser Zug am Griff das Blatt zu? Nur aus der Standardgröße heraus nach unten, sonst wird es erst wieder
/// kleiner.
pub fn closes_sheet(start_extra: f64, dy: f64) -> bool {
    start_extra <= 0.0 && dy > 56.0
}

/// Mindestweg, um einen Hinweis zur Seite wegzuwischen.
pub const DISMISS_SIDE: f64 = 64.0;
/// Mindestweg, um einen Hinweis nach oben wegzuwischen.
pub const DISMISS_UP: f64 = 40.0;

/// Wischt diese Bewegung einen Hinweis weg? Nach links, nach rechts oder nach oben.
pub fn dismisses(dx: f64, dy: f64) -> bool {
    (dx.abs() >= DISMISS_SIDE && dx.abs() > dy.abs()) || (dy <= -DISMISS_UP && dy.abs() > dx.abs())
}

/// Verschiebung des Hinweises, solange der Finger noch auf dem Glas liegt: Er folgt dem Finger in der
/// Hauptrichtung. Nach unten bewegt er sich nicht, damit Scrollen ihn nicht mitnimmt.
pub fn drag_offset(dx: f64, dy: f64) -> (f64, f64) {
    if dx.abs() >= dy.abs() {
        (dx, 0.0)
    } else {
        (0.0, dy.min(0.0))
    }
}

/// Deckkraft des Hinweises beim Ziehen: Je weiter weg, desto blasser.
pub fn drag_opacity(dx: f64, dy: f64) -> f64 {
    let (x, y) = drag_offset(dx, dy);
    let progress = (x.abs() / (DISMISS_SIDE * 2.0)).max(y.abs() / (DISMISS_UP * 2.0));
    (1.0 - progress).clamp(0.25, 1.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_sheet_grows_when_pulled_up_shrinks_when_pulled_down_and_snaps_back_to_default() {
        use super::{closes_sheet, sheet_extra_after_drag};
        // Nach oben ziehen macht das Blatt höher, höchstens bis zum Limit.
        assert_eq!(sheet_extra_after_drag(0.0, -150.0, 400.0), 150.0);
        assert_eq!(sheet_extra_after_drag(0.0, -900.0, 400.0), 400.0);
        // Nach unten ziehen macht es wieder kleiner, nie unter die Standardgröße.
        assert_eq!(sheet_extra_after_drag(300.0, 120.0, 400.0), 180.0);
        assert_eq!(sheet_extra_after_drag(100.0, 500.0, 400.0), 0.0);
        // Ein kleiner Rest rastet auf die Standardgröße ein.
        assert_eq!(sheet_extra_after_drag(0.0, -10.0, 400.0), 0.0);
        // Zu geht es nur aus der Standardgröße heraus.
        assert!(closes_sheet(0.0, 80.0));
        assert!(!closes_sheet(0.0, 30.0));
        assert!(!closes_sheet(200.0, 300.0));
    }

    #[test]
    fn dots_stand_still_in_pages_of_fifteen_and_move_on_after_the_last_one() {
        use super::dot_window;
        assert_eq!(dot_window(3, 10), 0..10);
        assert_eq!(dot_window(0, 62), 0..15);
        // Innerhalb einer Seite bleiben die Punkte stehen, nur der aktive wandert.
        assert_eq!(dot_window(7, 62), 0..15);
        assert_eq!(dot_window(14, 62), 0..15);
        // Ein Schritt nach dem letzten Punkt zeigt die nächste Seite.
        assert_eq!(dot_window(15, 62), 15..30);
        assert_eq!(dot_window(30, 62), 30..45);
        // Die letzte Seite ist kürzer, es wird nichts wiederholt.
        assert_eq!(dot_window(15, 17), 15..17);
        assert_eq!(dot_window(61, 62), 60..62);
        assert_eq!(dot_window(0, 0), 0..0);
        assert!(dot_window(40, 62).contains(&40));
    }

    use super::*;

    #[test]
    fn a_fast_horizontal_flick_changes_the_page() {
        assert_eq!(page_swipe(-120.0, 10.0, 180.0, 390.0), Some(Swipe::Left));
        assert_eq!(page_swipe(90.0, -20.0, 150.0, 390.0), Some(Swipe::Right));
    }

    #[test]
    fn scrolling_and_taps_do_not_change_the_page() {
        assert_eq!(page_swipe(-80.0, 200.0, 200.0, 390.0), None, "senkrechtes Scrollen");
        assert_eq!(page_swipe(-90.0, 60.0, 150.0, 390.0), None, "zu schräg");
        assert_eq!(page_swipe(-30.0, 2.0, 60.0, 390.0), None, "zu kurz");
        assert_eq!(page_swipe(0.0, 0.0, 0.0, 390.0), None);
        assert_eq!(page_swipe(f64::NAN, 0.0, 100.0, 390.0), None);
    }

    #[test]
    fn a_slow_drag_only_counts_when_it_goes_far() {
        assert_eq!(page_swipe(-100.0, 5.0, 900.0, 390.0), None, "langsam und kurz");
        assert_eq!(
            page_swipe(-170.0, 5.0, 900.0, 390.0),
            Some(Swipe::Left),
            "langsam, aber weit"
        );
        assert_eq!(
            page_swipe(-170.0, 5.0, 900.0, 0.0),
            None,
            "ohne bekannte Breite zählt nur das Tempo"
        );
    }

    #[test]
    fn swiping_moves_to_the_neighbor_and_stops_at_the_ends() {
        assert_eq!(neighbor(6, 0, Swipe::Left), Some(1));
        assert_eq!(neighbor(6, 4, Swipe::Left), Some(5));
        assert_eq!(neighbor(6, 5, Swipe::Left), None);
        assert_eq!(neighbor(6, 3, Swipe::Right), Some(2));
        assert_eq!(neighbor(6, 0, Swipe::Right), None);
        assert_eq!(neighbor(0, 0, Swipe::Left), None);
        assert_eq!(
            neighbor(3, 9, Swipe::Right),
            None,
            "eine Nummer außerhalb der Reihe hat keinen Nachbarn"
        );
    }

    #[test]
    fn hints_are_dismissed_sideways_or_upwards_but_not_downwards() {
        assert!(dismisses(-80.0, 5.0));
        assert!(dismisses(70.0, -10.0));
        assert!(dismisses(4.0, -50.0));
        assert!(!dismisses(4.0, 90.0), "nach unten ist Scrollen");
        assert!(!dismisses(30.0, 0.0), "zu kurz");
        assert!(dismisses(-70.0, -90.0), "schräg nach oben zählt als nach oben");
        assert!(!dismisses(-50.0, 60.0), "schräg nach unten zählt nicht");
    }

    #[test]
    fn the_ticker_advances_card_by_card_and_wraps_around() {
        // Drei Karten à 300 Pixel mit 12 Pixel Abstand: 924 Pixel insgesamt, Karten bei 0, 312 und 624.
        assert_eq!(next_card_offset(0.0, 300.0, 924.0), Some(312.0));
        assert_eq!(next_card_offset(312.0, 300.0, 924.0), Some(624.0));
        assert_eq!(
            next_card_offset(624.0, 300.0, 924.0),
            Some(0.0),
            "am Ende zurück zum Anfang"
        );
        assert_eq!(
            next_card_offset(130.0, 300.0, 924.0),
            Some(312.0),
            "halb gewischt: zur nächsten Karte"
        );
        // Weit hinten summiert sich der Abstand: Karte 7 (Nummer 6) liegt bei 6 mal 312.
        assert_eq!(next_card_offset(1872.0, 300.0, 6924.0), Some(2184.0));
        assert_eq!(next_card_offset(0.0, 300.0, 300.0), None, "nur eine Karte");
        assert_eq!(next_card_offset(0.0, 0.0, 900.0), None, "unsichtbar");
        assert_eq!(next_card_offset(f64::NAN, 300.0, 900.0), None);
    }

    #[test]
    fn the_back_arrow_goes_page_by_page_and_wraps_to_the_end() {
        assert_eq!(prev_card_offset(624.0, 300.0, 924.0), Some(312.0));
        assert_eq!(prev_card_offset(312.0, 300.0, 924.0), Some(0.0));
        assert_eq!(prev_card_offset(0.0, 300.0, 924.0), Some(624.0), "am Anfang: ans Ende");
        assert_eq!(prev_card_offset(0.0, 300.0, 300.0), None, "nur eine Karte");
        assert_eq!(prev_card_offset(f64::NAN, 300.0, 900.0), None);
    }

    #[test]
    fn a_page_offset_includes_the_gap_and_stops_at_the_end() {
        assert_eq!(page_offset(0, 300.0, 924.0), 0.0);
        assert_eq!(page_offset(2, 300.0, 924.0), 624.0);
        assert_eq!(page_offset(9, 300.0, 924.0), 624.0, "nie über das Ende hinaus");
        assert_eq!(page_offset(3, 0.0, 924.0), 0.0);
    }

    #[test]
    fn the_ticker_is_calm() {
        assert_eq!(TICKER_INTERVAL_MS, 10_000);
        const { assert!(TICKER_SLIDE_MS > ARROW_SLIDE_MS && TICKER_SLIDE_MS >= 1_000.0) };
    }

    #[test]
    fn the_card_in_view_is_found_from_the_scroll_position() {
        assert_eq!(card_index(0.0, 300.0), 0);
        assert_eq!(card_index(160.0, 300.0), 1);
        assert_eq!(card_index(624.0, 300.0), 2);
        assert_eq!(card_index(1872.0, 300.0), 6, "Karte 7 trotz summiertem Abstand");
        assert_eq!(card_index(10.0, 0.0), 0);
        assert_eq!(card_index(-5.0, 300.0), 0);
    }

    #[test]
    fn the_hint_follows_the_finger_in_its_main_direction() {
        assert_eq!(drag_offset(-40.0, 6.0), (-40.0, 0.0));
        assert_eq!(drag_offset(5.0, -30.0), (0.0, -30.0));
        assert_eq!(drag_offset(5.0, 30.0), (0.0, 0.0), "nach unten bleibt er liegen");
        assert_eq!(drag_opacity(0.0, 0.0), 1.0);
        assert!(drag_opacity(-64.0, 0.0) < 0.6);
        assert_eq!(drag_opacity(-500.0, 0.0), 0.25);
        assert!(drag_opacity(0.0, -40.0) < 0.6);
    }
}
