//! Icons aus dem Set Lucide (ISC-Lizenz, Dateien unter `frontend/icons`). Die Strichstärke steuert das CSS (1,5 px).

use leptos::prelude::*;

/// SVG-Text eines Icons, `None` bei unbekanntem Namen.
pub fn svg(name: &str) -> Option<&'static str> {
    Some(match name {
        "arrow-down" => include_str!("../icons/arrow-down.svg"),
        "arrow-up" => include_str!("../icons/arrow-up.svg"),
        "check" => include_str!("../icons/check.svg"),
        "download" => include_str!("../icons/download.svg"),
        "layout-dashboard" => include_str!("../icons/layout-dashboard.svg"),
        "map-pin" => include_str!("../icons/map-pin.svg"),
        "moon" => include_str!("../icons/moon.svg"),
        "search" => include_str!("../icons/search.svg"),
        "sun" => include_str!("../icons/sun.svg"),
        "triangle-alert" => include_str!("../icons/triangle-alert.svg"),
        "wrench" => include_str!("../icons/wrench.svg"),
        "x" => include_str!("../icons/x.svg"),
        _ => return None,
    })
}

#[component]
pub fn Icon(name: &'static str) -> impl IntoView {
    view! { <span class="icon" aria-hidden="true" inner_html=svg(name).unwrap_or_default()></span> }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_icon_exists_and_is_an_svg() {
        for name in [
            "arrow-down",
            "arrow-up",
            "check",
            "download",
            "layout-dashboard",
            "map-pin",
            "moon",
            "search",
            "sun",
            "triangle-alert",
            "wrench",
            "x",
        ] {
            let text = svg(name).unwrap_or_else(|| panic!("{name} fehlt"));
            assert!(text.contains("<svg") && text.contains("</svg>"), "{name}");
        }
    }

    #[test]
    fn unknown_icons_are_none() {
        assert_eq!(svg("gibts-nicht"), None);
    }
}
