//! Suche der Befehlspalette und Zuordnung der Tastenkürzel (rein, ohne Browser).
//!
//! Die Palette kann fast alles öffnen, zeigt aber bei leerer Suche nur die Hauptseiten: So bleibt sie kurz und braucht
//! keine Bildlaufleiste. Alles Weitere (Bereiche, Reiter, Abschnitte, Aktionen, Geräte) findet, wer tippt.

use crate::maintenance::MaintenanceTab;
use crate::overlays::Export;
use crate::state::Page;
use shared::DeviceStatus;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Page(Page),
    Device(String),
    /// Ein Reiter der Wartung.
    MaintenanceTab(MaintenanceTab),
    /// Ein Abschnitt einer Seite: Die Seite öffnen und zum Element mit dieser Kennung scrollen.
    Section(Page, &'static str),
    ToggleTheme,
    /// `auto`, `light` oder `dark`.
    SetTheme(&'static str),
    FilterStatus(DeviceStatus),
    ClearFilters,
    ExportCsv(Export),
    Help,
    SignOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub hint: String,
    pub target: Target,
    /// Steht schon bei leerer Suche in der Liste. Alles andere erscheint erst beim Tippen.
    pub main: bool,
}

impl Item {
    pub fn new(label: impl Into<String>, hint: impl Into<String>, target: Target) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
            target,
            main: false,
        }
    }

    /// Dieser Eintrag gehört zur Liste bei leerer Suche.
    pub fn main(mut self) -> Self {
        self.main = true;
        self
    }
}

/// Kennungen der Abschnitte, die die Palette ansteuert. Jede steht als `id` in der jeweiligen Seite, ein Test prüft das.
pub mod anchor {
    pub const TODO: &str = "todo-box";
    pub const DEVICE_FILTER: &str = "device-toolbar";
    pub const EFF_STOCK: &str = "eff-stock";
    pub const EFF_INVALID: &str = "invalid-log";
    pub const EFF_COMPARE: &str = "eff-compare";
    pub const EFF_MODELS: &str = "eff-models";
    pub const OPT_APPEARANCE: &str = "opt-appearance";
    pub const OPT_AVATAR: &str = "opt-avatar";
    pub const OPT_THRESHOLDS: &str = "opt-thresholds";
    pub const OPT_ANOMALIES: &str = "opt-anomalies";
    pub const OPT_LOCATION: &str = "opt-location";
    pub const OPT_TRASH: &str = "opt-trash";
    pub const OPT_LAB: &str = "opt-lab";
    pub const PROFILE_PASSWORD: &str = "profile-password";

    /// Alle Kennungen, zur Prüfung im Test.
    #[cfg(test)]
    pub const ALL: [&str; 14] = [
        TODO,
        DEVICE_FILTER,
        EFF_STOCK,
        EFF_INVALID,
        EFF_COMPARE,
        EFF_MODELS,
        OPT_APPEARANCE,
        OPT_AVATAR,
        OPT_THRESHOLDS,
        OPT_ANOMALIES,
        OPT_LOCATION,
        OPT_TRASH,
        OPT_LAB,
        PROFILE_PASSWORD,
    ];
}

/// Name des Wartungs-Reiters in der Palette. Zwei heißen dort genauer als auf dem Reiter selbst.
fn tab_label(tab: MaintenanceTab) -> &'static str {
    match tab {
        MaintenanceTab::Batteries => "Batterie-Katalog",
        MaintenanceTab::History => "Wartungsprotokoll",
        other => other.label(),
    }
}

/// Alle Einträge, die nicht von den Daten abhängen: Seiten, Reiter, Abschnitte und Aktionen. `export_all` schaltet die
/// CSV-Aktionen frei (Zusatzfunktion), `admin` die Kontenverwaltung. Nur die Seiten der Navigation sind Hauptseiten.
pub fn command_items(export_all: bool, admin: bool) -> Vec<Item> {
    let mut list = Vec::new();
    for page in Page::ALL {
        if page == Page::Accounts && !admin {
            continue;
        }
        let item = Item::new(page.label(), "Seite", Target::Page(page));
        list.push(if Page::NAV.contains(&page) { item.main() } else { item });
    }
    for tab in MaintenanceTab::ALL {
        list.push(Item::new(
            tab_label(tab),
            "Wartung, Reiter",
            Target::MaintenanceTab(tab),
        ));
    }
    let sections: [(&str, Page, &'static str); 13] = [
        ("Offene Meldungen", Page::Overview, anchor::TODO),
        ("Geräte suchen und filtern", Page::Overview, anchor::DEVICE_FILTER),
        ("Datenhaltung", Page::Efficiency, anchor::EFF_STOCK),
        ("Verworfene Nachrichten", Page::Efficiency, anchor::EFF_INVALID),
        ("Geräte im Vergleich", Page::Efficiency, anchor::EFF_COMPARE),
        ("Batteriemodelle im Vergleich", Page::Efficiency, anchor::EFF_MODELS),
        ("Darstellung", Page::Options, anchor::OPT_APPEARANCE),
        ("Profilbild wählen", Page::Options, anchor::OPT_AVATAR),
        ("Schwellwerte", Page::Options, anchor::OPT_THRESHOLDS),
        ("Auffälligkeiten", Page::Options, anchor::OPT_ANOMALIES),
        ("Standardstandort", Page::Options, anchor::OPT_LOCATION),
        ("Papierkorb", Page::Options, anchor::OPT_TRASH),
        ("Labor, Zusatzfunktionen", Page::Options, anchor::OPT_LAB),
    ];
    for (label, page, id) in sections {
        list.push(Item::new(
            label,
            format!("{}, Abschnitt", page.label()),
            Target::Section(page, id),
        ));
    }
    list.push(Item::new(
        "Passwort ändern",
        "Profil, Abschnitt",
        Target::Section(Page::Profile, anchor::PROFILE_PASSWORD),
    ));
    list.push(Item::new("Farbschema wechseln", "Aktion, Taste T", Target::ToggleTheme));
    list.push(Item::new("Farbschema hell", "Aktion", Target::SetTheme("light")));
    list.push(Item::new("Farbschema dunkel", "Aktion", Target::SetTheme("dark")));
    list.push(Item::new(
        "Farbschema automatisch",
        "Aktion, wie das Gerät",
        Target::SetTheme("auto"),
    ));
    for (label, status) in [
        ("Nur kritische Geräte zeigen", DeviceStatus::Critical),
        ("Nur Warnungen zeigen", DeviceStatus::Warning),
        ("Nur Offline-Geräte zeigen", DeviceStatus::Offline),
        ("Nur normale Geräte zeigen", DeviceStatus::Normal),
    ] {
        list.push(Item::new(label, "Aktion", Target::FilterStatus(status)));
    }
    list.push(Item::new("Filter zurücksetzen", "Aktion", Target::ClearFilters));
    if export_all {
        list.push(Item::new(
            "Alle Geräte als CSV exportieren",
            "Aktion",
            Target::ExportCsv(Export::All),
        ));
        list.push(Item::new(
            "Gefilterte Geräte als CSV exportieren",
            "Aktion",
            Target::ExportCsv(Export::Filtered),
        ));
        list.push(Item::new(
            "Merkliste als CSV exportieren",
            "Aktion",
            Target::ExportCsv(Export::Pinned),
        ));
    }
    list.push(Item::new("Tastenkürzel anzeigen", "Hilfe, Taste ?", Target::Help));
    list.push(Item::new("Abmelden", "Aktion", Target::SignOut));
    list
}

/// Klein geschrieben und ohne Umlaute und Akzente: "Größe" und "grosse" sind dasselbe.
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .flat_map(|c| match c {
            'ä' | 'à' | 'á' | 'â' => vec!['a'],
            'ö' | 'ò' | 'ó' | 'ô' => vec!['o'],
            'ü' | 'ù' | 'ú' | 'û' => vec!['u'],
            'é' | 'è' | 'ê' => vec!['e'],
            'ß' => vec!['s', 's'],
            other => vec![other],
        })
        .collect()
}

/// Wertung einer Teilfolgen-Suche ohne Beachtung der Groß-/Kleinschreibung und der Umlaute. `None`, wenn nicht alle
/// Zeichen der Eingabe der Reihe nach vorkommen. Höher ist besser: Treffer am Anfang, am Wortanfang und in einem
/// Stück zählen mehr, ein zusammenhängendes Stück der Eingabe zählt am meisten.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = fold(query).chars().filter(|c| !c.is_whitespace()).collect();
    if query.is_empty() {
        return Some(0);
    }
    let folded = fold(text);
    let chars: Vec<char> = folded.chars().collect();
    let mut total = 0;
    let mut position = 0;
    let mut previous: Option<usize> = None;
    for &q in &query {
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
    // Steht die ganze Eingabe als zusammenhängendes Stück im Text (zum Beispiel "dome" in "Dometic"), ist es der beste Treffer.
    let joined: String = query.iter().collect();
    if folded.contains(&joined) {
        total += 20;
    }
    Some(total)
}

/// Beste Treffer zuerst, bei gleicher Wertung in der Reihenfolge der Eingabe. Eine leere Suche liefert nur die
/// Hauptseiten (`Item::main`), damit die Liste ohne Bildlauf auskommt.
pub fn search(items: &[Item], query: &str, limit: usize) -> Vec<Item> {
    if query.trim().is_empty() {
        return items.iter().filter(|item| item.main).take(limit).cloned().collect();
    }
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
        "5" => Some(KeyAction::Page(4)),
        "t" | "T" => Some(KeyAction::ToggleTheme),
        "?" => Some(KeyAction::Help),
        _ => None,
    }
}

pub const SHORTCUTS: [(&str, &str); 9] = [
    ("Strg + K", "Befehlspalette öffnen"),
    ("/", "Geräte suchen"),
    ("1", "Übersicht"),
    ("2", "Standortkarte"),
    ("3", "Wartung"),
    ("4", "Postfach"),
    ("5", "Effizienz"),
    ("T", "Farbschema wechseln"),
    ("Esc", "Schließen"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Page;

    fn item(label: &str, hint: &str, target: Target) -> Item {
        Item::new(label, hint, target)
    }

    #[test]
    fn the_keys_one_to_five_switch_exactly_the_five_navigation_pages() {
        for (key, index) in [("1", 0), ("2", 1), ("3", 2), ("4", 3), ("5", 4)] {
            assert_eq!(key_action(key, false, false), Some(KeyAction::Page(index)), "{key}");
            assert!(index < Page::NAV.len());
        }
        assert_eq!(Page::NAV.len(), 5);
        assert_eq!(key_action("6", false, false), None, "Optionen liegen hinter dem Profil");
        assert_eq!(key_action("0", false, false), None);
    }

    #[test]
    fn digits_do_nothing_while_typing() {
        for key in ["1", "2", "3", "4", "5"] {
            assert_eq!(key_action(key, false, true), None, "{key}");
        }
    }

    #[test]
    fn the_help_lists_every_page_shortcut() {
        for (number, page) in Page::NAV.iter().enumerate() {
            let label = page.label();
            let key = (number + 1).to_string();
            assert!(
                SHORTCUTS.iter().any(|(k, text)| *k == key && *text == label),
                "{key} {label}"
            );
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
    fn umlauts_and_case_do_not_matter() {
        assert!(score("gross", "Großer Test").is_some());
        assert!(score("MÜNCHEN", "munchen").is_some());
        assert!(score("uber", "Über die Brücke").is_some());
    }

    #[test]
    fn a_word_start_is_found_inside_a_longer_name() {
        assert!(score("dome", "Dometic NDS GreenPower GP80 (AGM, 12 V, 80 Ah)").is_some());
        assert!(
            score("dome", "Dometic NDS GreenPower").unwrap() > score("dome", "Dom Elektro").unwrap(),
            "ein zusammenhängendes Stück zählt mehr als verstreute Buchstaben"
        );
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
        assert!(search(&items, "zzz", 5).is_empty());
    }

    #[test]
    fn an_empty_search_lists_only_the_main_pages() {
        let items = vec![
            item("Standortkarte", "Seite", Target::Page(Page::Map)).main(),
            item("SIM03", "Erfunden, kritisch", Target::Device("SIM03".into())),
            item(
                "Schwellwerte",
                "Optionen, Abschnitt",
                Target::Section(Page::Options, "opt-thresholds"),
            ),
            item("Wartung", "Seite", Target::Page(Page::Maintenance)).main(),
        ];
        let labels = |query: &str| {
            search(&items, query, 8)
                .into_iter()
                .map(|i| i.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(labels(""), ["Standortkarte", "Wartung"]);
        assert_eq!(
            labels("   "),
            ["Standortkarte", "Wartung"],
            "Leerzeichen zählen nicht als Eingabe"
        );
        assert_eq!(search(&items, "", 1).len(), 1);
        // Sobald getippt wird, ist alles auffindbar.
        assert_eq!(labels("schwell"), ["Schwellwerte"]);
        assert_eq!(labels("sim"), ["SIM03"]);
    }

    #[test]
    fn the_empty_list_is_exactly_the_navigation_and_fits_without_a_scrollbar() {
        let items = command_items(true, true);
        let main: Vec<&Item> = items.iter().filter(|i| i.main).collect();
        assert_eq!(
            main.iter().map(|i| i.target.clone()).collect::<Vec<_>>(),
            Page::NAV.iter().map(|p| Target::Page(*p)).collect::<Vec<_>>()
        );
        assert!(main.len() <= 6, "mehr Zeilen brauchen eine Bildlaufleiste");
    }

    #[test]
    fn nearly_every_feature_can_be_opened_by_typing() {
        let items = command_items(true, true);
        let first = |query: &str| search(&items, query, 8).into_iter().next().map(|i| i.target);
        assert_eq!(first("optionen"), Some(Target::Page(Page::Options)));
        assert_eq!(first("konten"), Some(Target::Page(Page::Accounts)));
        assert_eq!(
            first("batterie-kat"),
            Some(Target::MaintenanceTab(MaintenanceTab::Batteries))
        );
        assert_eq!(first("merkl"), Some(Target::MaintenanceTab(MaintenanceTab::Pins)));
        assert_eq!(
            first("schwellw"),
            Some(Target::Section(Page::Options, anchor::OPT_THRESHOLDS))
        );
        assert_eq!(
            first("passwort"),
            Some(Target::Section(Page::Profile, anchor::PROFILE_PASSWORD))
        );
        assert_eq!(first("dunkel"), Some(Target::SetTheme("dark")));
        assert_eq!(first("warnungen"), Some(Target::FilterStatus(DeviceStatus::Warning)));
        assert_eq!(first("abmelden"), Some(Target::SignOut));
        assert_eq!(first("merkliste als csv"), Some(Target::ExportCsv(Export::Pinned)));
    }

    #[test]
    fn accounts_and_csv_are_only_offered_when_allowed() {
        let has = |items: &[Item], target: Target| items.iter().any(|i| i.target == target);
        let plain = command_items(false, false);
        assert!(!has(&plain, Target::Page(Page::Accounts)));
        assert!(!has(&plain, Target::ExportCsv(Export::All)));
        let full = command_items(true, true);
        assert!(has(&full, Target::Page(Page::Accounts)));
        assert!(has(&full, Target::ExportCsv(Export::All)));
        // Jede Seite ist erreichbar.
        for page in Page::ALL {
            assert!(has(&full, Target::Page(page)), "{page:?}");
        }
    }

    #[test]
    fn every_entry_has_a_unique_label_and_every_anchor_exists_in_a_page() {
        let items = command_items(true, true);
        let mut labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count, "doppelte Bezeichnung");
        assert!(items.iter().all(|i| !i.label.is_empty() && !i.hint.is_empty()));

        let sources = [
            include_str!("overview.rs"),
            include_str!("efficiency_view.rs"),
            include_str!("maintenance.rs"),
            include_str!("avatar.rs"),
            include_str!("trash.rs"),
            include_str!("profile.rs"),
        ];
        for id in anchor::ALL {
            assert!(
                sources.iter().any(|source| source.contains(&format!("id=\"{id}\""))),
                "Kennung {id} fehlt in den Seiten"
            );
        }
        for item in &items {
            if let Target::Section(_, id) = item.target {
                assert!(anchor::ALL.contains(&id), "{id} fehlt in anchor::ALL");
            }
        }
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
