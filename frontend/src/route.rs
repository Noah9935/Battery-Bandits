//! Adressen der Oberfläche. Die fünf Hauptseiten bleiben unter `/`, ein einzelnes Gerät hat eine eigene,
//! teilbare Adresse: `/geraete/TN0001`. Der Webserver liefert dafür auf jedem Pfad die Oberfläche aus.

/// Ein Gerät, so wie es in einer Adresse stehen darf: Buchstaben, Ziffern, Unterstrich, Punkt und Bindestrich.
pub fn is_valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Adresse der Geräteseite, `None` bei einer Kennung, die sich nicht sauber in eine Adresse schreiben lässt.
pub fn device_path(id: &str) -> Option<String> {
    is_valid_device_id(id).then(|| format!("/geraete/{id}"))
}

/// Gerätekennung aus einem Pfad: `/geraete/TN0001` und `/geraete/TN0001/` gelten, alles andere nicht.
pub fn device_from_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/geraete/")?;
    let id = rest.trim_end_matches('/');
    is_valid_device_id(id).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_path_roundtrips() {
        for id in ["TN0001", "SIM01", "a-b_c.d"] {
            let path = device_path(id).unwrap();
            assert_eq!(path, format!("/geraete/{id}"));
            assert_eq!(device_from_path(&path).as_deref(), Some(id));
        }
    }

    #[test]
    fn a_trailing_slash_is_fine() {
        assert_eq!(device_from_path("/geraete/TN0001/").as_deref(), Some("TN0001"));
    }

    #[test]
    fn other_paths_are_not_device_pages() {
        for path in [
            "/",
            "",
            "/geraete",
            "/geraete/",
            "/geraete//",
            "/geraete/TN0001/extra",
            "/andere/TN0001",
            "geraete/TN0001",
        ] {
            assert_eq!(device_from_path(path), None, "{path}");
        }
    }

    #[test]
    fn ids_with_odd_characters_get_no_address() {
        for id in ["", "a b", "a/b", "ä", "x?y", "x#y", "a%20b"] {
            assert_eq!(device_path(id), None, "{id}");
            assert!(!is_valid_device_id(id));
        }
        assert_eq!(device_path(&"x".repeat(65)), None);
    }
}
