//! Profilbilder: 100 verschiedene Waschbären. Jedes Konto bekommt bei der Anlage zufällig eine Nummer von 0 bis 99
//! zugewiesen (auf dem Server), das Bild entsteht hier als SVG aus zehn Hintergründen und zehn Ausstattungen.
//! Alle Zusammensetzungen sind verschieden, kein Bild liegt als Datei vor.

use leptos::prelude::*;

/// Zahl der Bilder.
pub const COUNT: u8 = shared::api::AVATAR_COUNT;

const BACKGROUNDS: [&str; 10] = [
    "#f6e9d9", "#dfe8f2", "#e2f0e4", "#f3dfe3", "#ece6f5", "#f7efc9", "#d9efef", "#f0e0d0", "#e6e4e0", "#d8d6f0",
];

/// Fellfarben: Jeder Waschbär ist ein wenig anders grau.
const FUR: [&str; 5] = ["#8c867e", "#7d7770", "#9a948b", "#857d75", "#a39d94"];

/// Ausstattungen, in dieser Reihenfolge: nichts, Brille, Sonnenbrille, Partyhut, Mütze, Fliege, Schal, Kopfhörer,
/// Blume, Bauhelm.
const ACCESSORIES: [&str; 10] = [
    "",
    r##"<g fill="none" stroke="#c97a1e" stroke-width="2.2"><circle cx="37" cy="52" r="9"/><circle cx="63" cy="52" r="9"/><path d="M46 52h8"/></g>"##,
    r##"<g fill="#111"><rect x="26" y="45" width="21" height="14" rx="5"/><rect x="53" y="45" width="21" height="14" rx="5"/><rect x="46" y="48" width="8" height="2.5"/></g>"##,
    r##"<path d="M50 3 L35 33 H65 Z" fill="#e0527a"/><circle cx="50" cy="4" r="4" fill="#f5c542"/><path d="M40 25 H60 M44 17 H56" stroke="#fff" stroke-width="2"/>"##,
    r##"<path d="M18 36 Q50 -6 82 36 Z" fill="#3b6fb6"/><rect x="16" y="31" width="68" height="9" rx="4.5" fill="#2b528a"/><circle cx="50" cy="8" r="5" fill="#e8e1d6"/>"##,
    r##"<path d="M50 90 L35 81 V99 Z M50 90 L65 81 V99 Z" fill="#c4261c"/><circle cx="50" cy="90" r="4" fill="#8f1a14"/>"##,
    r##"<path d="M16 83 Q50 99 84 83 L86 94 Q50 110 14 94 Z" fill="#b4661c"/><path d="M30 91 v10 M42 94 v10 M54 95 v10 M66 93 v10" stroke="#f6e9d9" stroke-width="2"/>"##,
    r##"<path d="M16 55 Q16 14 50 14 Q84 14 84 55" fill="none" stroke="#2b2926" stroke-width="5"/><rect x="9" y="46" width="14" height="24" rx="6" fill="#e3a55b"/><rect x="77" y="46" width="14" height="24" rx="6" fill="#e3a55b"/>"##,
    r##"<g transform="translate(76 31)"><circle cx="0" cy="-6" r="4.5" fill="#f4a6c0"/><circle cx="6" cy="0" r="4.5" fill="#f4a6c0"/><circle cx="0" cy="6" r="4.5" fill="#f4a6c0"/><circle cx="-6" cy="0" r="4.5" fill="#f4a6c0"/><circle r="3.5" fill="#f5c542"/></g>"##,
    r##"<path d="M20 37 Q22 7 50 7 Q78 7 80 37 Z" fill="#f5c542"/><rect x="14" y="34" width="72" height="7" rx="3.5" fill="#d9a721"/><rect x="45" y="7" width="10" height="27" fill="#d9a721"/>"##,
];

/// Das Bild mit dieser Nummer als SVG-Text. Nummern ab 100 beginnen von vorn.
pub fn raccoon_svg(id: u8) -> String {
    let id = usize::from(id % COUNT);
    let background = BACKGROUNDS[id % 10];
    let accessory = ACCESSORIES[id / 10];
    let fur = FUR[(id * 7 + id / 10) % FUR.len()];
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" role="img" aria-label="Waschbär"><rect width="100" height="100" fill="{background}"/><circle cx="25" cy="31" r="13" fill="{fur}"/><circle cx="75" cy="31" r="13" fill="{fur}"/><circle cx="25" cy="31" r="6.5" fill="#2b2926"/><circle cx="75" cy="31" r="6.5" fill="#2b2926"/><ellipse cx="50" cy="57" rx="35" ry="31" fill="{fur}"/><ellipse cx="50" cy="68" rx="23" ry="17" fill="#ece8e2"/><path d="M17 50 Q50 34 83 50 Q77 66 62 61 Q50 55 38 61 Q23 66 17 50Z" fill="#2b2926"/><circle cx="37" cy="52" r="4.6" fill="#fff"/><circle cx="63" cy="52" r="4.6" fill="#fff"/><circle cx="37.6" cy="52.4" r="2.3" fill="#1a1816"/><circle cx="62.4" cy="52.4" r="2.3" fill="#1a1816"/><ellipse cx="50" cy="69" rx="5.5" ry="4" fill="#2b2926"/><path d="M44 76 Q50 81 56 76" fill="none" stroke="#2b2926" stroke-width="2" stroke-linecap="round"/>{accessory}</svg>"##
    )
}

/// Rundes Profilbild.
#[component]
pub fn Avatar(id: u8, #[prop(optional, into)] class: String) -> impl IntoView {
    view! { <span class=format!("avatar-img {class}") aria-hidden="true" inner_html=raccoon_svg(id)></span> }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn all_hundred_raccoons_are_different() {
        let all: BTreeSet<String> = (0..COUNT).map(raccoon_svg).collect();
        assert_eq!(all.len(), 100);
        assert_eq!(COUNT, 100);
    }

    #[test]
    fn every_picture_is_a_closed_svg() {
        for id in 0..COUNT {
            let svg = raccoon_svg(id);
            assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"), "{id}");
            assert_eq!(
                svg.matches("<g ").count(),
                svg.matches("</g>").count(),
                "{id}: Gruppen offen"
            );
            assert!(svg.contains("Waschbär"), "{id}");
        }
    }

    #[test]
    fn numbers_beyond_the_last_start_over() {
        assert_eq!(raccoon_svg(100), raccoon_svg(0));
        assert_eq!(raccoon_svg(255), raccoon_svg(55));
    }

    #[test]
    fn background_and_equipment_are_both_visible_in_the_choice() {
        // Zehn Nachbarn haben dieselbe Ausstattung und zehn verschiedene Hintergründe, und umgekehrt.
        let backgrounds: BTreeSet<&str> = (0..10).map(|id| BACKGROUNDS[id % 10]).collect();
        assert_eq!(backgrounds.len(), 10);
        assert_eq!(ACCESSORIES.iter().collect::<BTreeSet<_>>().len(), 10);
        assert!(raccoon_svg(9).contains(BACKGROUNDS[9]) && raccoon_svg(10).contains(BACKGROUNDS[0]));
        assert!(!raccoon_svg(0).contains("stroke=\"#c97a1e\""), "Bild 0 trägt nichts");
        assert!(raccoon_svg(10).contains("#c97a1e"), "Bild 10 trägt eine Brille");
        assert!(raccoon_svg(90).contains("#f5c542"), "Bild 90 trägt einen Bauhelm");
    }
}
