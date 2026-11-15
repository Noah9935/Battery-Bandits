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

/// Wohin die waagerechte Leiste (Ticker) als Nächstes scrollt. Am Ende geht es zurück zum Anfang.
/// `None`, wenn nichts zu scrollen ist (eine Karte oder unsichtbar).
pub fn next_card_offset(left: f64, width: f64, total: f64) -> Option<f64> {
    if !(left.is_finite() && width.is_finite() && total.is_finite()) || width <= 0.0 || total <= width + 1.0 {
        return None;
    }
    let last = total - width;
    if left + 1.0 >= last {
        return Some(0.0);
    }
    Some((((left / width).round() + 1.0) * width).min(last))
}

/// Wohin die Leiste beim Pfeil nach links scrollt. Am Anfang geht es ans Ende.
pub fn prev_card_offset(left: f64, width: f64, total: f64) -> Option<f64> {
    if !(left.is_finite() && width.is_finite() && total.is_finite()) || width <= 0.0 || total <= width + 1.0 {
        return None;
    }
    let last = total - width;
    if left <= 1.0 {
        return Some(last);
    }
    Some((((left / width).round() - 1.0) * width).max(0.0))
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
    (left / width).round() as usize
}

/// So viele Punkte zeigt die Leiste unter den Karten höchstens. Bei mehr Karten wandert ein Fenster mit.
pub const MAX_DOTS: usize = 15;

/// Welche Karten als Punkte erscheinen: höchstens [`MAX_DOTS`], das Fenster folgt der aktuellen Karte.
pub fn dot_window(current: usize, total: usize) -> std::ops::Range<usize> {
    if total <= MAX_DOTS {
        return 0..total;
    }
    let start = current.saturating_sub(MAX_DOTS / 2).min(total - MAX_DOTS);
    start..start + MAX_DOTS
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
    fn at_most_fifteen_dots_are_shown_and_the_window_follows_the_card() {
        use super::dot_window;
        assert_eq!(dot_window(3, 10), 0..10);
        assert_eq!(dot_window(0, 62), 0..15);
        assert_eq!(dot_window(30, 62), 23..38);
        assert_eq!(dot_window(61, 62), 47..62);
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
        assert_eq!(next_card_offset(0.0, 300.0, 900.0), Some(300.0));
        assert_eq!(next_card_offset(300.0, 300.0, 900.0), Some(600.0));
        assert_eq!(
            next_card_offset(600.0, 300.0, 900.0),
            Some(0.0),
            "am Ende zurück zum Anfang"
        );
        assert_eq!(
            next_card_offset(130.0, 300.0, 900.0),
            Some(300.0),
            "halb gewischt: zur nächsten Karte"
        );
        assert_eq!(next_card_offset(0.0, 300.0, 300.0), None, "nur eine Karte");
        assert_eq!(next_card_offset(0.0, 0.0, 900.0), None, "unsichtbar");
        assert_eq!(next_card_offset(f64::NAN, 300.0, 900.0), None);
    }

    #[test]
    fn the_back_arrow_goes_page_by_page_and_wraps_to_the_end() {
        assert_eq!(prev_card_offset(600.0, 300.0, 900.0), Some(300.0));
        assert_eq!(prev_card_offset(300.0, 300.0, 900.0), Some(0.0));
        assert_eq!(prev_card_offset(0.0, 300.0, 900.0), Some(600.0), "am Anfang: ans Ende");
        assert_eq!(prev_card_offset(0.0, 300.0, 300.0), None, "nur eine Karte");
        assert_eq!(prev_card_offset(f64::NAN, 300.0, 900.0), None);
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
        assert_eq!(card_index(600.0, 300.0), 2);
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
