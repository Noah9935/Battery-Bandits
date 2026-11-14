//! Profilbilder: 100 echte Waschbär-Fotos. Jedes Konto bekommt bei der Anlage zufällig eine Nummer von 0 bis 99
//! zugewiesen (auf dem Server). Die Fotos liegen quadratisch zugeschnitten unter `frontend/avatars/00.jpg` bis `99.jpg`,
//! Urheber und Lizenz stehen in `frontend/avatars/BILDNACHWEIS.txt` (in den Optionen verlinkt).

use leptos::prelude::*;

/// Zahl der Bilder.
pub const COUNT: u8 = shared::api::AVATAR_COUNT;

/// Adresse des Fotos mit dieser Nummer. Nummern ab 100 beginnen von vorn.
pub fn avatar_url(id: u8) -> String {
    format!("/avatars/{:02}.jpg", id % COUNT)
}

/// Rundes Profilbild.
#[component]
pub fn Avatar(id: u8, #[prop(optional, into)] class: String) -> impl IntoView {
    view! {
        <span class=format!("avatar-img {class}") aria-hidden="true">
            <img src=avatar_url(id) alt="" width="96" height="96" loading="lazy" decoding="async"/>
        </span>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_number_has_its_own_photo() {
        let all: BTreeSet<String> = (0..COUNT).map(avatar_url).collect();
        assert_eq!(all.len(), 100);
        assert_eq!(COUNT, 100);
        assert_eq!(avatar_url(0), "/avatars/00.jpg");
        assert_eq!(avatar_url(99), "/avatars/99.jpg");
    }

    #[test]
    fn numbers_beyond_the_last_start_over() {
        assert_eq!(avatar_url(100), avatar_url(0));
        assert_eq!(avatar_url(255), avatar_url(55));
    }

    #[test]
    fn the_photos_and_their_credits_are_part_of_the_project() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("avatars");
        for id in 0..COUNT {
            let file = dir.join(format!("{id:02}.jpg"));
            let bytes = std::fs::read(&file).unwrap_or_else(|_| panic!("{} fehlt", file.display()));
            assert!(bytes.starts_with(&[0xFF, 0xD8]), "{} ist kein JPEG", file.display());
        }
        let credits = std::fs::read_to_string(dir.join("BILDNACHWEIS.txt")).expect("Bildnachweis fehlt");
        assert_eq!(
            credits.lines().filter(|l| l.starts_with("Bild ")).count(),
            100,
            "je Bild ein Eintrag"
        );
    }
}
