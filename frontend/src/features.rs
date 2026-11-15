//! Schaltbare Zusatzfunktionen ("Labor"). Jede lässt sich einzeln abschalten, die Wahl bleibt im Browser.
//! Neue Funktionen sind vorerst alle an, damit man sie durchgehen und die uncoolen abschalten kann.

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    Palette,
    Shortcuts,
    Sparklines,
    Activity,
    Favorites,
    Crosshair,
    Ring,
    ExportAll,
    Deeplink,
    Notify,
}

impl Feature {
    pub const ALL: [Feature; 10] = [
        Feature::Palette,
        Feature::Shortcuts,
        Feature::Sparklines,
        Feature::Activity,
        Feature::Favorites,
        Feature::Crosshair,
        Feature::Ring,
        Feature::ExportAll,
        Feature::Deeplink,
        Feature::Notify,
    ];

    /// Kennung für die Speicherung im Browser.
    pub fn id(self) -> &'static str {
        match self {
            Feature::Palette => "palette",
            Feature::Shortcuts => "shortcuts",
            Feature::Sparklines => "sparklines",
            Feature::Activity => "activity",
            Feature::Favorites => "favorites",
            Feature::Crosshair => "crosshair",
            Feature::Ring => "ring",
            Feature::ExportAll => "export-all",
            Feature::Deeplink => "deeplink",
            Feature::Notify => "notify",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Feature::Palette => "Befehlspalette",
            Feature::Shortcuts => "Tastenkürzel",
            Feature::Sparklines => "Trendlinien in der Tabelle",
            Feature::Activity => "Aktivitätsprotokoll",
            Feature::Favorites => "Geräte anpinnen",
            Feature::Crosshair => "Fadenkreuz im Diagramm",
            Feature::Ring => "Ladezustands-Ring",
            Feature::ExportAll => "CSV-Export der Geräte",
            Feature::Deeplink => "Teilbare Filter-Links",
            Feature::Notify => "Meldungen im Tab und Browser",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Feature::Palette => "Strg+K öffnet eine Suche für Geräte, Seiten und Aktionen.",
            Feature::Shortcuts => "Taste / sucht, 1 bis 5 wechseln die Seite, T das Farbschema, ? zeigt alle Kürzel.",
            Feature::Sparklines => "Kleine Verlaufskurve des Ladezustands der letzten 24 Stunden in jeder Zeile.",
            Feature::Activity => "Live-Liste der Statuswechsel, Meldungen und Batteriewechsel dieser Sitzung.",
            Feature::Favorites => "Merken-Knopf an jedem Gerät, dazu die schnellste Route zu allen angepinnten.",
            Feature::Crosshair => "Wert und Zeit unter dem Mauszeiger im Verlaufsdiagramm.",
            Feature::Ring => "Runde Anzeige des Ladezustands oben in den Gerätedetails.",
            Feature::ExportAll => "Alle Geräte, die gefilterte Liste oder die eigene Merkliste als CSV herunterladen.",
            Feature::Deeplink => "Filter und Sortierung stehen im Link und lassen sich teilen.",
            Feature::Notify => "Zahl offener Meldungen im Tab-Titel, optional Browser-Benachrichtigung bei kritisch.",
        }
    }
}

/// Alle Funktionen, die nicht ausdrücklich abgeschaltet wurden. `stored` ist die Liste abgeschalteter Kennungen
/// (kommagetrennt), unbekannte Einträge werden ignoriert.
pub fn parse_enabled(stored: &str) -> BTreeSet<Feature> {
    let off: BTreeSet<&str> = stored.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    Feature::ALL.into_iter().filter(|f| !off.contains(f.id())).collect()
}

/// Gegenstück zu [`parse_enabled`]: speichert die abgeschalteten Kennungen.
pub fn serialize_disabled(enabled: &BTreeSet<Feature>) -> String {
    Feature::ALL
        .into_iter()
        .filter(|f| !enabled.contains(f))
        .map(Feature::id)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_on_by_default() {
        assert_eq!(parse_enabled("").len(), Feature::ALL.len());
        assert_eq!(serialize_disabled(&parse_enabled("")), "");
    }

    #[test]
    fn disabled_ids_roundtrip() {
        let enabled = parse_enabled("sparklines, notify");
        assert!(!enabled.contains(&Feature::Sparklines));
        assert!(!enabled.contains(&Feature::Notify));
        assert!(enabled.contains(&Feature::Palette));
        assert_eq!(serialize_disabled(&enabled), "sparklines,notify");
    }

    #[test]
    fn unknown_ids_are_ignored_and_new_features_default_on() {
        assert_eq!(parse_enabled("gibt-es-nicht").len(), Feature::ALL.len());
    }

    #[test]
    fn ids_are_unique_and_texts_present() {
        let ids: BTreeSet<&str> = Feature::ALL.iter().map(|f| f.id()).collect();
        assert_eq!(ids.len(), Feature::ALL.len());
        for f in Feature::ALL {
            assert!(!f.label().is_empty() && !f.description().is_empty());
            assert!(
                !f.label().contains(['\u{2013}', '\u{2014}']) && !f.description().contains(['\u{2013}', '\u{2014}'])
            );
        }
    }

    #[test]
    fn the_shortcut_text_names_all_five_pages() {
        let text = Feature::Shortcuts.description();
        assert!(text.contains("1 bis 5") && !text.contains("1 bis 6"), "{text}");
    }
}
