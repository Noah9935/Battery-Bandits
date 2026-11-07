//! Suche der Befehlspalette und Zuordnung der Tastenkürzel (rein, ohne Browser).

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Page(crate::state::Page),
    Device(String),
    ToggleTheme,
    FilterCritical,
    ClearFilters,
    ExportCsv,
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub hint: String,
    pub target: Target,
}

/// Wertung einer Teilfolgen-Suche ohne Beachtung der Groß-/Kleinschreibung. `None`, wenn nicht alle Zeichen der
/// Eingabe der Reihe nach vorkommen. Höher ist besser: Treffer am Anfang, am Wortanfang und in einem Stück zählen mehr.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if query.is_empty() {
        return Some(0);
    }
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut total = 0;
    let mut position = 0;
    let mut previous: Option<usize> = None;
    for q in query {
        let found = (position..chars.len()).find(|&i| chars[i] == q)?;
        total += 10;
        if found == 0 || !chars[found - 1].is_alphanumeric() {
            total += 8;
        }
        if previous == Some(found.wrapping_sub(1)) {
            total += 6;
        }
        total -= (found - position).min(10) as i32;
        previous = Some(found);
        position = found + 1;
    }
    Some(total)
}

/// Beste Treffer zuerst, bei gleicher Wertung in der Reihenfolge der Eingabe. Leere Suche liefert den Anfang der Liste.
pub fn search(items: &[Item], query: &str, limit: usize) -> Vec<Item> {
    let mut scored: Vec<(i32, usize, &Item)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            let by_label = score(query, &item.label);
            let by_hint = score(query, &item.hint).map(|s| s - 5);
            by_label.into_iter().chain(by_hint).max().map(|s| (s, i, item))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, item)| item.clone())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Palette,
    FocusSearch,
    Page(usize),
    ToggleTheme,
    Help,
    Close,
}

/// Welche Aktion gehört zu einer Taste? Beim Tippen in einem Feld wirken nur Strg+K und Escape.
pub fn key_action(key: &str, ctrl_or_meta: bool, typing: bool) -> Option<KeyAction> {
    if ctrl_or_meta {
        return key.eq_ignore_ascii_case("k").then_some(KeyAction::Palette);
    }
    if key == "Escape" {
        return Some(KeyAction::Close);
    }
    if typing {
        return None;
    }
    match key {
        "/" => Some(KeyAction::FocusSearch),
        "1" => Some(KeyAction::Page(0)),
        "2" => Some(KeyAction::Page(1)),
        "3" => Some(KeyAction::Page(2)),
        "4" => Some(KeyAction::Page(3)),
        "t" | "T" => Some(KeyAction::ToggleTheme),
        "?" => Some(KeyAction::Help),
        _ => None,
    }
}

pub const SHORTCUTS: [(&str, &str); 8] = [
    ("Strg + K", "Befehlspalette öffnen"),
    ("/", "Geräte suchen"),
    ("1", "Übersicht"),
    ("2", "Standortkarte"),
    ("3", "Wartung"),
    ("4", "Optionen"),
    ("T", "Farbschema wechseln"),
    ("Esc", "Schließen"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Page;

    fn item(label: &str, hint: &str, target: Target) -> Item {
        Item {
            label: label.into(),
            hint: hint.into(),
            target,
        }
    }

    #[test]
    fn score_requires_all_characters_in_order() {
        assert!(score("tn1", "TN0001").is_some());
        assert!(score("1nt", "TN0001").is_none());
        assert_eq!(score("", "egal"), Some(0));
        assert!(score("xyz", "TN0001").is_none());
    }

    #[test]
    fn prefix_and_contiguous_matches_beat_scattered_ones() {
        assert!(score("sim", "SIM03").unwrap() > score("sim", "Schlossplatz im Morgen").unwrap());
        assert!(score("kar", "Standortkarte").unwrap() > score("kte", "Standortkarte").unwrap());
    }

    #[test]
    fn search_orders_by_score_and_respects_limit() {
        let items = vec![
            item("Standortkarte", "Seite", Target::Page(Page::Map)),
            item("SIM03", "Erfunden, kritisch", Target::Device("SIM03".into())),
            item("SIM12", "Erfunden, Warnung", Target::Device("SIM12".into())),
            item("Wartung", "Seite", Target::Page(Page::Maintenance)),
        ];
        let result = search(&items, "sim", 5);
        assert_eq!(
            result.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["SIM03", "SIM12"]
        );
        assert_eq!(search(&items, "", 2).len(), 2);
        assert!(search(&items, "zzz", 5).is_empty());
    }

    #[test]
    fn search_also_matches_the_hint() {
        let items = vec![item("TSN-002", "Dozent, offline", Target::Device("TSN-002".into()))];
        assert_eq!(search(&items, "offline", 3).len(), 1);
    }

    #[test]
    fn shortcuts_map_to_actions() {
        assert_eq!(key_action("k", true, false), Some(KeyAction::Palette));
        assert_eq!(
            key_action("K", true, true),
            Some(KeyAction::Palette),
            "Strg+K wirkt auch im Eingabefeld"
        );
        assert_eq!(key_action("/", false, false), Some(KeyAction::FocusSearch));
        assert_eq!(key_action("2", false, false), Some(KeyAction::Page(1)));
        assert_eq!(key_action("4", false, false), Some(KeyAction::Page(3)));
        assert_eq!(key_action("t", false, false), Some(KeyAction::ToggleTheme));
        assert_eq!(key_action("?", false, false), Some(KeyAction::Help));
        assert_eq!(key_action("x", false, false), None);
    }

    #[test]
    fn typing_blocks_plain_keys_but_not_escape() {
        assert_eq!(key_action("t", false, true), None);
        assert_eq!(key_action("/", false, true), None);
        assert_eq!(key_action("Escape", false, true), Some(KeyAction::Close));
    }

    #[test]
    fn every_listed_shortcut_has_text() {
        assert!(SHORTCUTS.iter().all(|(k, d)| !k.is_empty() && !d.is_empty()));
    }
}
