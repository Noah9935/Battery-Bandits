//! Reihenfolge der angepinnten Geräte für die Fahrt (rein, ohne Browser).
//!
//! Gesucht ist die kürzeste Strecke vom eigenen Standort zum ersten Gerät und weiter zu den übrigen, ohne
//! Rückfahrt. Bis acht Halte wird jede Reihenfolge durchprobiert, das Ergebnis ist also die beste. Darüber wird
//! mit "immer zum nächsten" begonnen und so lange nachgebessert, bis das Umdrehen von Teilstrecken nichts mehr
//! bringt. Gerechnet wird mit Luftlinie: Die echte Fahrstrecke berechnet danach Google Maps.

use crate::tour::{Leg, Stop, haversine_km};

/// Bis zu dieser Zahl von Halten wird die beste Reihenfolge durch Ausprobieren gefunden (8! = 40320 Möglichkeiten).
pub const EXACT_LIMIT: usize = 8;

type Point = (f64, f64);

/// Länge der Strecke in Kilometern: vom Start (falls bekannt) der Reihe nach durch alle Punkte.
pub fn path_km(start: Option<Point>, points: &[Point], order: &[usize]) -> f64 {
    let mut here = start;
    let mut total = 0.0;
    for &i in order {
        let Some(next) = points.get(i).copied() else {
            continue;
        };
        if let Some(from) = here {
            total += haversine_km(from, next);
        }
        here = Some(next);
    }
    total
}

/// Probiert alle Reihenfolgen durch (Tiefensuche). Ein Zweig wird abgebrochen, sobald er schon länger ist als
/// die beste bisher gefundene Strecke.
fn search(
    here: Option<Point>,
    points: &[Point],
    used: &mut [bool],
    order: &mut Vec<usize>,
    length: f64,
    best: &mut (f64, Vec<usize>),
) {
    if order.len() == points.len() {
        if length < best.0 {
            *best = (length, order.clone());
        }
        return;
    }
    for i in 0..points.len() {
        if used[i] {
            continue;
        }
        let step = here.map_or(0.0, |from| haversine_km(from, points[i]));
        if length + step >= best.0 {
            continue;
        }
        used[i] = true;
        order.push(i);
        search(Some(points[i]), points, used, order, length + step, best);
        order.pop();
        used[i] = false;
    }
}

/// Immer zum nächsten noch offenen Punkt. Ohne Start beginnt die Reihe beim ersten Punkt.
fn nearest_neighbor(start: Option<Point>, points: &[Point]) -> Vec<usize> {
    let mut open: Vec<usize> = (0..points.len()).collect();
    let mut order = Vec::with_capacity(points.len());
    let mut here = start;
    while !open.is_empty() {
        let pick = match here {
            Some(from) => open
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| haversine_km(from, points[**a]).total_cmp(&haversine_km(from, points[**b])))
                .map_or(0, |(position, _)| position),
            None => 0,
        };
        let index = open.remove(pick);
        here = Some(points[index]);
        order.push(index);
    }
    order
}

/// Dreht Teilstrecken um, solange die Gesamtstrecke dadurch kürzer wird (2-opt für eine offene Strecke).
fn improve(start: Option<Point>, points: &[Point], mut order: Vec<usize>) -> Vec<usize> {
    let mut best = path_km(start, points, &order);
    let mut improved = true;
    while improved {
        improved = false;
        for from in 0..order.len() {
            for to in from + 1..order.len() {
                order[from..=to].reverse();
                let length = path_km(start, points, &order);
                if length + 1e-9 < best {
                    best = length;
                    improved = true;
                } else {
                    order[from..=to].reverse();
                }
            }
        }
    }
    order
}

/// Beste Reihenfolge der Punkte. Zurück kommen ihre Nummern in der Reihenfolge der Anfahrt.
pub fn best_order(start: Option<Point>, points: &[Point]) -> Vec<usize> {
    if points.len() <= 1 {
        return (0..points.len()).collect();
    }
    let greedy = nearest_neighbor(start, points);
    if points.len() > IMPROVE_LIMIT {
        return greedy;
    }
    if points.len() > EXACT_LIMIT {
        return improve(start, points, greedy);
    }
    // Die Strecke von "immer zum nächsten" ist die Messlatte: Alles, was länger wird, bricht früh ab.
    let mut best = (path_km(start, points, &greedy) + 1e-9, greedy);
    search(
        start,
        points,
        &mut vec![false; points.len()],
        &mut Vec::with_capacity(points.len()),
        0.0,
        &mut best,
    );
    best.1
}

/// Die Halte in der besten Reihenfolge, mit der Strecke zum jeweils vorherigen Halt (beim ersten zum Start).
pub fn ordered_legs(start: Option<Point>, stops: Vec<Stop>) -> Vec<Leg> {
    let points: Vec<Point> = stops.iter().map(|s| (s.lat, s.lon)).collect();
    let order = best_order(start, &points);
    let mut slots: Vec<Option<Stop>> = stops.into_iter().map(Some).collect();
    let mut here = start;
    let mut legs = Vec::with_capacity(order.len());
    for index in order {
        let Some(stop) = slots.get_mut(index).and_then(Option::take) else {
            continue;
        };
        let km = here.map(|from| haversine_km(from, (stop.lat, stop.lon)));
        here = Some((stop.lat, stop.lon));
        legs.push(Leg { stop, km });
    }
    legs
}

/// Über dieser Zahl bleibt es bei "immer zum nächsten": Das Nachbessern kostet sonst zu viel Rechenzeit, und Google
/// Maps nimmt ohnehin nur elf Punkte.
pub const IMPROVE_LIMIT: usize = 60;

/// Ist die Reihenfolge durch Ausprobieren gefunden (und damit die beste) oder eine gute Näherung?
pub fn is_exact(stops: usize) -> bool {
    stops <= EXACT_LIMIT
}

/// Standort als Text für den Speicher im Browser: `48.775800,9.182900`.
pub fn encode_point(point: Point) -> String {
    format!("{:.6},{:.6}", point.0, point.1)
}

/// Gegenstück zu [`encode_point`]. Unsinnige Werte ergeben `None`.
pub fn parse_point(text: &str) -> Option<Point> {
    let (lat, lon) = text.trim().split_once(',')?;
    let (lat, lon) = (lat.trim().parse::<f64>().ok()?, lon.trim().parse::<f64>().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Punkte auf dem Äquator bei den angegebenen Längengraden.
    fn line(lons: &[f64]) -> Vec<Point> {
        lons.iter().map(|lon| (0.0, *lon)).collect()
    }

    fn stop(id: &str, lon: f64) -> Stop {
        Stop {
            device_id: id.into(),
            title: id.into(),
            lat: 0.0,
            lon,
            reason: String::new(),
            urgent: false,
        }
    }

    #[test]
    fn the_legs_follow_the_best_order_and_measure_each_hop() {
        let legs = ordered_legs(Some((0.0, 0.0)), vec![stop("C", 3.0), stop("A", 1.0), stop("B", 2.0)]);
        let ids: Vec<&str> = legs.iter().map(|l| l.stop.device_id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C"]);
        for leg in &legs {
            assert!((leg.km.unwrap() - 111.19).abs() < 0.2);
        }
        let without_start = ordered_legs(None, vec![stop("B", 2.0), stop("A", 1.0)]);
        assert_eq!(without_start[0].km, None, "ohne Start hat der erste Halt keine Anfahrt");
        assert!(ordered_legs(None, Vec::new()).is_empty());
    }

    #[test]
    fn nothing_and_one_point_are_trivial() {
        assert!(best_order(None, &[]).is_empty());
        assert_eq!(best_order(Some((0.0, 0.0)), &[(1.0, 1.0)]), [0]);
        assert_eq!(path_km(None, &[], &[]), 0.0);
    }

    #[test]
    fn points_on_a_line_are_visited_in_order_from_the_start() {
        let points = line(&[3.0, 1.0, 2.0, 5.0, 4.0]);
        assert_eq!(best_order(Some((0.0, 0.0)), &points), [1, 2, 0, 4, 3]);
        // Vom anderen Ende aus andersherum.
        assert_eq!(best_order(Some((0.0, 9.0)), &points), [3, 4, 0, 2, 1]);
    }

    #[test]
    fn trying_everything_beats_always_going_to_the_nearest() {
        // Start bei 0. Der nächste Punkt liegt knapp links (-1), alle anderen rechts. "Immer zum nächsten" fährt
        // erst nach links und dann den ganzen Weg zurück (1 + 11 = 12 Grad). Besser: rechts beginnen und links
        // enden wäre 10 + 11 = 21, schlechter. Hier ist "zuerst links" wirklich am besten.
        let simple = line(&[-1.0, 10.0]);
        assert_eq!(best_order(Some((0.0, 0.0)), &simple), [0, 1]);

        // Jetzt lockt der nächste Punkt in die falsche Richtung: Start bei 0, Punkte bei 2, -3 und -3,5.
        // Zum nächsten zuerst (2, dann -3, dann -3,5) sind 2 + 5 + 0,5 = 7,5 Grad.
        // Erst nach links und zuletzt nach rechts (-3, -3,5, 2) wären 3 + 0,5 + 5,5 = 9 Grad.
        let lure = line(&[2.0, -3.0, -3.5]);
        let order = best_order(Some((0.0, 0.0)), &lure);
        let greedy = nearest_neighbor(Some((0.0, 0.0)), &lure);
        assert!(path_km(Some((0.0, 0.0)), &lure, &order) <= path_km(Some((0.0, 0.0)), &lure, &greedy) + 1e-9);

        // Ein Fall, in dem "immer zum nächsten" nachweislich verliert: Start 0, Punkte bei 1, -2, 3, -4 ...
        let zigzag = line(&[1.0, -2.0, 3.0, -4.0, 5.0]);
        let start = Some((0.0, 0.0));
        let exact = path_km(start, &zigzag, &best_order(start, &zigzag));
        let nearest = path_km(start, &zigzag, &nearest_neighbor(start, &zigzag));
        assert!(exact < nearest - 100.0, "{exact} gegen {nearest}");
        // Die beste Lösung fährt erst die kurze Seite ab und dann die lange: -2, -4, dann 1, 3, 5.
        assert_eq!(best_order(start, &zigzag), [1, 3, 0, 2, 4]);
    }

    #[test]
    fn the_exact_search_is_never_worse_than_any_other_order() {
        let points = [
            (48.78, 9.18),
            (48.80, 9.01),
            (48.70, 9.30),
            (48.90, 9.20),
            (48.75, 9.10),
            (48.83, 9.25),
        ];
        let start = Some((48.77, 9.17));
        let best = path_km(start, &points, &best_order(start, &points));
        // Stichprobe anderer Reihenfolgen, darunter die Eingabereihenfolge und ihre Umkehrung.
        for other in [
            vec![0, 1, 2, 3, 4, 5],
            vec![5, 4, 3, 2, 1, 0],
            vec![4, 0, 5, 3, 1, 2],
            vec![1, 4, 0, 2, 5, 3],
            nearest_neighbor(start, &points),
        ] {
            assert!(best <= path_km(start, &points, &other) + 1e-9, "{other:?}");
        }
    }

    #[test]
    fn every_stop_is_visited_exactly_once() {
        for count in [2, 5, 8, 9, 14] {
            let points: Vec<Point> = (0..count)
                .map(|i| {
                    let i = f64::from(i);
                    (48.0 + (i * 0.37).sin() * 0.4, 9.0 + (i * 0.91).cos() * 0.6)
                })
                .collect();
            let mut order = best_order(Some((48.0, 9.0)), &points);
            assert_eq!(order.len(), points.len());
            order.sort_unstable();
            assert_eq!(order, (0..points.len()).collect::<Vec<_>>(), "{count} Halte");
        }
    }

    #[test]
    fn many_stops_are_improved_beyond_the_greedy_tour() {
        // Zehn Punkte, mehr als die Grenze für das Ausprobieren. Der nächste Punkt (bei 1) lockt nach rechts,
        // dann geht es zur linken Gruppe und den ganzen Weg zurück zur rechten: 1 + 3 + 2 + 10 + 1,5 = 17,5 Grad.
        // Besser ist: erst die linke Gruppe, dann nach rechts durch: 4 + 5 + 5 + 1,5 = 15,5 Grad.
        let points = line(&[1.0, -2.0, -2.5, -3.0, -3.5, -4.0, 6.0, 6.5, 7.0, 7.5]);
        let start = Some((0.0, 0.0));
        let improved = path_km(start, &points, &best_order(start, &points));
        let greedy = path_km(start, &points, &nearest_neighbor(start, &points));
        assert!((greedy - 17.5 * 111.19).abs() < 2.0, "{greedy}");
        assert!((improved - 15.5 * 111.19).abs() < 2.0, "{improved}");
        assert!(!is_exact(points.len()) && is_exact(8));
    }

    #[test]
    fn without_a_start_the_shorter_end_to_end_route_wins() {
        let points = line(&[5.0, 1.0, 3.0]);
        let order = best_order(None, &points);
        // Von einem Ende zum anderen: 1, 3, 5 oder umgekehrt. Beides ist gleich lang und am kürzesten.
        assert!(order == [1, 2, 0] || order == [0, 2, 1], "{order:?}");
        assert!((path_km(None, &points, &order) - 4.0 * 111.19).abs() < 1.0);
    }

    #[test]
    fn unknown_indexes_are_skipped_when_measuring() {
        let points = line(&[1.0, 2.0]);
        assert_eq!(
            path_km(Some((0.0, 0.0)), &points, &[0, 7, 1]),
            path_km(Some((0.0, 0.0)), &points, &[0, 1])
        );
    }

    #[test]
    fn the_saved_location_roundtrips_and_rejects_nonsense() {
        let home = (48.7758, 9.1829);
        assert_eq!(encode_point(home), "48.775800,9.182900");
        assert_eq!(parse_point(&encode_point(home)), Some(home));
        assert_eq!(parse_point(" 48.5 , 9.25 "), Some((48.5, 9.25)));
        for bad in ["", "48.5", "a,b", "91,9", "48,181", "48;9"] {
            assert_eq!(parse_point(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_huge_list_is_ordered_quickly_without_the_improvement_step() {
        let points: Vec<Point> = (0..400)
            .map(|i| {
                let i = f64::from(i);
                (48.0 + (i * 0.37).sin() * 0.4, 9.0 + (i * 0.91).cos() * 0.6)
            })
            .collect();
        let mut order = best_order(Some((48.0, 9.0)), &points);
        assert_eq!(order.len(), 400);
        order.sort_unstable();
        order.dedup();
        assert_eq!(order.len(), 400, "jeder Halt genau einmal");
    }
}
