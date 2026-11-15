//! Touren und Merkliste auf dem Server: Jede Person hat ihre eigene Merkliste, Touren (benannte Listen von Nodes,
//! bei denen man die Batterie wechseln will) lassen sich anderen Konten freigeben, damit sie die Route abgehen.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Längster Name einer Tour.
pub const MAX_TOUR_NAME_CHARS: usize = 60;
/// Wie viele Nodes eine Tour höchstens hat.
pub const MAX_TOUR_NODES: usize = 100;
/// Wie viele Personen eine Tour höchstens sehen dürfen.
pub const MAX_TOUR_SHARES: usize = 50;

/// Eine Tour, wie die Oberfläche sie sieht: eigene und die, die andere mit einem geteilt haben.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Tour {
    pub id: i64,
    pub name: String,
    /// Benutzername der Person, die die Tour angelegt hat.
    pub owner: String,
    /// Anzeigename dieser Person.
    pub owner_name: String,
    /// Die Nodes der Tour in der Reihenfolge, in der sie eingetragen wurden.
    pub device_ids: Vec<String>,
    /// Mit wem die Tour geteilt ist. Nur die Besitzerin oder der Besitzer sieht das, sonst ist die Liste leer.
    pub shared_with: Vec<String>,
    /// Gehört die Tour der anfragenden Person? Nur dann darf sie die Tour ändern, teilen und löschen.
    pub mine: bool,
    pub updated_at: DateTime<Utc>,
}

/// So lange bleibt eine gelöschte Tour im Papierkorb, bevor sie endgültig verschwindet.
pub const TRASH_DAYS: i64 = 30;

/// Eine gelöschte Tour im Papierkorb.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct TrashedTour {
    pub id: i64,
    pub name: String,
    /// Wie viele Nodes die Tour hatte.
    pub node_count: i32,
    pub deleted_at: DateTime<Utc>,
}

impl TrashedTour {
    /// Wie viele ganze Tage die Tour noch im Papierkorb bleibt (mindestens 0).
    pub fn days_left(&self, now: DateTime<Utc>) -> i64 {
        (TRASH_DAYS - (now - self.deleted_at).num_days()).max(0)
    }
}

/// Eine neue Tour, optional gleich mit anderen Konten geteilt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewTour {
    pub name: String,
    #[serde(default)]
    pub device_ids: Vec<String>,
    #[serde(default)]
    pub share_with: Vec<String>,
}

/// Änderung einer Tour: Was fehlt, bleibt, wie es ist.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TourUpdate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub device_ids: Option<Vec<String>>,
}

/// Mit wem die Tour geteilt sein soll. Die Liste ersetzt die bisherige.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ShareTour {
    #[serde(default)]
    pub usernames: Vec<String>,
}

/// Mehrere Nodes für die Merkliste.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PinIds {
    #[serde(default)]
    pub ids: Vec<String>,
}

/// Name einer Tour ohne Leerraum an den Rändern. Er darf nicht leer und nicht zu lang sein.
pub fn clean_tour_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Bitte der Tour einen Namen geben, zum Beispiel Dienstag Nord.");
    }
    if name.chars().count() > MAX_TOUR_NAME_CHARS {
        return Err("Der Name ist zu lang (höchstens 60 Zeichen).");
    }
    Ok(name.to_owned())
}

/// Kennungen ohne Leerraum, ohne leere und ohne doppelte Einträge, in der ersten Reihenfolge.
pub fn clean_ids(ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    ids.iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && seen.insert((*id).to_owned()))
        .map(str::to_owned)
        .collect()
}

/// Nodes einer Tour prüfen und bereinigen.
pub fn clean_tour_devices(ids: &[String]) -> Result<Vec<String>, &'static str> {
    let ids = clean_ids(ids);
    if ids.len() > MAX_TOUR_NODES {
        return Err("Eine Tour kann höchstens 100 Nodes haben.");
    }
    Ok(ids)
}

impl NewTour {
    /// Prüft Namen und Nodes. Gibt den bereinigten Namen und die bereinigten Nodes zurück.
    pub fn cleaned(&self) -> Result<(String, Vec<String>), &'static str> {
        Ok((clean_tour_name(&self.name)?, clean_tour_devices(&self.device_ids)?))
    }
}

/// Mit wem geteilt wird, ohne Doppelte und ohne die Besitzerin oder den Besitzer selbst.
pub fn clean_shares(usernames: &[String], owner: &str) -> Result<Vec<String>, &'static str> {
    let list: Vec<String> = clean_ids(usernames).into_iter().filter(|u| u != owner).collect();
    if list.len() > MAX_TOUR_SHARES {
        return Err("Eine Tour lässt sich mit höchstens 50 Personen teilen.");
    }
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_tour_needs_a_name_of_sensible_length() {
        assert_eq!(clean_tour_name("  Dienstag Nord "), Ok("Dienstag Nord".to_owned()));
        assert!(clean_tour_name("   ").unwrap_err().contains("Namen"));
        assert!(clean_tour_name(&"x".repeat(61)).unwrap_err().contains("zu lang"));
        assert!(clean_tour_name(&"x".repeat(60)).is_ok());
    }

    #[test]
    fn nodes_lose_blanks_and_duplicates_but_keep_their_order() {
        assert_eq!(
            clean_ids(&strings(&[" SIM02", "SIM01", "", "SIM02 ", "  "])),
            ["SIM02", "SIM01"]
        );
        let too_many: Vec<String> = (0..101).map(|i| format!("N{i}")).collect();
        assert!(clean_tour_devices(&too_many).is_err());
        assert_eq!(clean_tour_devices(&too_many[..100]).unwrap().len(), 100);
    }

    #[test]
    fn sharing_skips_the_owner_and_doubles() {
        assert_eq!(
            clean_shares(&strings(&["fynn", "noah", "fynn", " "]), "noah").unwrap(),
            ["fynn"]
        );
        let many: Vec<String> = (0..51).map(|i| format!("u{i}")).collect();
        assert!(clean_shares(&many, "noah").is_err());
    }

    #[test]
    fn a_new_tour_is_checked_as_a_whole() {
        let ok = NewTour {
            name: " Süd ".into(),
            device_ids: strings(&["A", "A", "B"]),
            share_with: Vec::new(),
        };
        assert_eq!(ok.cleaned().unwrap(), ("Süd".to_owned(), strings(&["A", "B"])));
        assert!(NewTour::default().cleaned().is_err());
    }
}

#[cfg(test)]
mod trash_tests {
    use super::*;

    #[test]
    fn a_trashed_tour_counts_down_its_days() {
        let at = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let tour = TrashedTour {
            id: 1,
            name: "Dienstag".into(),
            node_count: 3,
            deleted_at: at,
        };
        assert_eq!(tour.days_left(at), 30);
        assert_eq!(tour.days_left(at + chrono::Duration::days(10)), 20);
        assert_eq!(tour.days_left(at + chrono::Duration::days(45)), 0);
    }
}
