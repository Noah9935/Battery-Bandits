//! Icons aus dem Set Lucide (ISC-Lizenz, Dateien unter `frontend/icons`). Die Strichstärke steuert das CSS (1,5 px).

use leptos::prelude::*;

/// SVG-Text eines Icons, `None` bei unbekanntem Namen.
pub fn svg(name: &str) -> Option<&'static str> {
    Some(match name {
        "arrow-down" => include_str!("../icons/arrow-down.svg"),
        "arrow-up" => include_str!("../icons/arrow-up.svg"),
        "check" => include_str!("../icons/check.svg"),
        "command" => include_str!("../icons/command.svg"),
        "download" => include_str!("../icons/download.svg"),
        "layout-dashboard" => include_str!("../icons/layout-dashboard.svg"),
        "map-pin" => include_str!("../icons/map-pin.svg"),
        "monitor" => include_str!("../icons/monitor.svg"),
        "moon" => include_str!("../icons/moon.svg"),
        "search" => include_str!("../icons/search.svg"),
        "settings" => include_str!("../icons/settings.svg"),
        "star" => include_str!("../icons/star.svg"),
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
            "command",
            "download",
            "layout-dashboard",
            "map-pin",
            "monitor",
            "moon",
            "search",
            "settings",
            "star",
            "sun",
            "triangle-alert",
            "wrench",
            "x",
        ] {
            let text = svg(name).unwrap_or_else(|| panic!("{name} fehlt"));
            assert!(text.contains("<svg") && text.contains("</svg>"), "{name}");
        }
    }

    /// Fängt vergessene Einträge ab: Jeder Name in `<Icon name="..."/>` oder `icon_name("...")` im Quelltext muss existieren.
    #[test]
    fn every_icon_used_in_the_sources_exists() {
        let sources = [
            include_str!("app.rs"),
            include_str!("overview.rs"),
            include_str!("drawer.rs"),
            include_str!("overlays.rs"),
            include_str!("maintenance.rs"),
        ];
        for source in sources {
            for part in source.split("name=\"").skip(1) {
                let name = part.split('"').next().unwrap();
                assert!(svg(name).is_some(), "Icon {name} fehlt in icons.rs");
            }
        }
        for name in [
            "layout-dashboard",
            "map-pin",
            "wrench",
            "settings",
            "sun",
            "moon",
            "monitor",
        ] {
            assert!(svg(name).is_some(), "{name}");
        }
    }

    #[test]
    fn unknown_icons_are_none() {
        assert_eq!(svg("gibts-nicht"), None);
    }
}
