//! Icons aus den Sets Lucide (ISC-Lizenz) und Feather (MIT-Lizenz), Dateien unter `frontend/icons`.
//! Die Strichstärke steuert das CSS.

use leptos::prelude::*;

/// SVG-Text eines Icons, `None` bei unbekanntem Namen.
pub fn svg(name: &str) -> Option<&'static str> {
    Some(match name {
        "activity" => include_str!("../icons/activity.svg"),
        "arrow-down" => include_str!("../icons/arrow-down.svg"),
        "arrow-left" => include_str!("../icons/arrow-left.svg"),
        "arrow-up" => include_str!("../icons/arrow-up.svg"),
        "arrow-up-right" => include_str!("../icons/arrow-up-right.svg"),
        "bell" => include_str!("../icons/bell.svg"),
        "bookmark" => include_str!("../icons/bookmark.svg"),
        "check" => include_str!("../icons/check.svg"),
        "chevron-down" => include_str!("../icons/chevron-down.svg"),
        "chevron-left" => include_str!("../icons/chevron-left.svg"),
        "chevron-right" => include_str!("../icons/chevron-right.svg"),
        "chevron-up" => include_str!("../icons/chevron-up.svg"),
        "command" => include_str!("../icons/command.svg"),
        "download" => include_str!("../icons/download.svg"),
        "filter" => include_str!("../icons/filter.svg"),
        "inbox" => include_str!("../icons/inbox.svg"),
        "layout-dashboard" => include_str!("../icons/layout-dashboard.svg"),
        "locate" => include_str!("../icons/locate.svg"),
        "log-out" => include_str!("../icons/log-out.svg"),
        "mail" => include_str!("../icons/mail.svg"),
        "map-pin" => include_str!("../icons/map-pin.svg"),
        "message-circle" => include_str!("../icons/message-circle.svg"),
        "monitor" => include_str!("../icons/monitor.svg"),
        "moon" => include_str!("../icons/moon.svg"),
        "navigation" => include_str!("../icons/navigation.svg"),
        "search" => include_str!("../icons/search.svg"),
        "send" => include_str!("../icons/send.svg"),
        "settings" => include_str!("../icons/settings.svg"),
        "share" => include_str!("../icons/share.svg"),
        "shuffle" => include_str!("../icons/shuffle.svg"),
        "smartphone" => include_str!("../icons/smartphone.svg"),
        "star" => include_str!("../icons/star.svg"),
        "sun" => include_str!("../icons/sun.svg"),
        "trash" => include_str!("../icons/trash.svg"),
        "triangle-alert" => include_str!("../icons/triangle-alert.svg"),
        "undo" => include_str!("../icons/undo.svg"),
        "user" => include_str!("../icons/user.svg"),
        "wrench" => include_str!("../icons/wrench.svg"),
        "x" => include_str!("../icons/x.svg"),
        "zap" => include_str!("../icons/zap.svg"),
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
            "activity",
            "arrow-down",
            "arrow-left",
            "arrow-up",
            "arrow-up-right",
            "bell",
            "bookmark",
            "check",
            "chevron-down",
            "chevron-left",
            "chevron-right",
            "chevron-up",
            "command",
            "download",
            "filter",
            "inbox",
            "layout-dashboard",
            "locate",
            "log-out",
            "mail",
            "map-pin",
            "message-circle",
            "monitor",
            "moon",
            "navigation",
            "search",
            "send",
            "settings",
            "share",
            "smartphone",
            "star",
            "sun",
            "triangle-alert",
            "user",
            "wrench",
            "x",
            "zap",
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
            include_str!("device.rs"),
            include_str!("efficiency_view.rs"),
            include_str!("forward.rs"),
            include_str!("inbox.rs"),
            include_str!("login.rs"),
            include_str!("maintenance.rs"),
            include_str!("map_view.rs"),
            include_str!("overlays.rs"),
            include_str!("overview.rs"),
            include_str!("pins.rs"),
            include_str!("swap.rs"),
            include_str!("tickets.rs"),
            include_str!("tour.rs"),
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
