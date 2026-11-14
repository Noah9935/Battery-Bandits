//! Geräte anpinnen ("Merken") und die schnellste Fahrt zu allen angepinnten über Google Maps.
//!
//! Der Start ist der eigene Standort (vom Browser, nach Erlaubnis), sonst der hinterlegte Standardstandort. Ohne
//! beides beginnt die Route beim ersten Gerät. Die Reihenfolge berechnet `routing`, die Strecke selbst Google Maps.

use std::collections::BTreeSet;

use leptos::prelude::*;
use shared::Device;

use crate::device::DeviceLink;
use crate::features::Feature;
use crate::icons::Icon;
use crate::routing::{is_exact, ordered_legs};
use crate::state::Store;
use crate::swap::SwapButton;
use crate::tour::{Leg, Stop, maps_url};
use crate::{
    format,
    overview::{SimBadge, StatusDot},
};

/// Angepinnte Geräte, die es gibt, in der Reihenfolge der Kennung.
pub fn pinned_devices(devices: &[Device], pinned: &BTreeSet<String>) -> Vec<Device> {
    let mut list: Vec<Device> = devices.iter().filter(|d| pinned.contains(&d.id)).cloned().collect();
    list.sort_by(|a, b| a.id.cmp(&b.id));
    list
}

/// Halte der Route: angepinnte Geräte mit bekannter Position.
pub fn pinned_stops(devices: &[Device]) -> Vec<Stop> {
    devices
        .iter()
        .filter_map(|d| {
            Some(Stop {
                device_id: d.id.clone(),
                title: format::device_title(&d.id, d.name.as_deref()),
                lat: d.lat?,
                lon: d.lon?,
                reason: format::status_label(d.status).to_owned(),
                urgent: d.status.needs_attention(),
            })
        })
        .collect()
}

/// Wie der Start der Route beschrieben wird.
pub fn start_text(has_position: bool, has_home: bool) -> &'static str {
    match (has_position, has_home) {
        (true, _) => "Start: dein Standort",
        (false, true) => "Start: Standardstandort",
        (false, false) => "Start: das erste Gerät der Route",
    }
}

/// Die Route als Rechenergebnis: Reihenfolge und Link.
pub fn route_memos(s: Store) -> (Memo<Vec<Leg>>, Memo<Option<String>>) {
    let legs = Memo::new(move |_| {
        let start = s.start_point();
        let stops = s
            .devices
            .with(|d| s.favorites.with(|f| pinned_stops(&pinned_devices(d, f))));
        ordered_legs(start, stops)
    });
    let url = Memo::new(move |_| {
        let start = s.start_point();
        legs.with(|l| maps_url(start, l))
    });
    (legs, url)
}

/// Knopf zum Anpinnen. Ein Tipp merkt das Gerät, der nächste löst es wieder.
#[component]
pub fn PinButton(id: String, #[prop(optional)] label: bool) -> impl IntoView {
    let s = expect_context::<Store>();
    let class_id = id.clone();
    let text_id = id.clone();
    let click_id = id;
    view! {
        {move || {
            s.on(Feature::Favorites)
                .then(|| {
                    let (class_id, text_id, click_id) = (class_id.clone(), text_id.clone(), click_id.clone());
                    let pinned = move || s.favorites.with(|f| f.contains(&class_id));
                    let pinned_for_text = pinned.clone();
                    let pinned_for_aria = pinned.clone();
                    view! {
                        <button
                            class=move || if pinned() { "pin-btn is-pinned" } else { "pin-btn" }
                            aria-pressed=move || pinned_for_aria().to_string()
                            aria-label=move || {
                                if s.favorites.with(|f| f.contains(&text_id)) { "Nicht mehr merken" } else { "Merken" }
                            }
                            title="Merken: das Gerät kommt auf die Merkliste und in die Route"
                            on:click=move |ev| {
                                ev.stop_propagation();
                                s.toggle_pin(&click_id);
                            }
                        >
                            <Icon name="bookmark"/>
                            {label.then(|| view! { <span>{move || if pinned_for_text() { "Gemerkt" } else { "Merken" }}</span> })}
                        </button>
                    }
                })
        }}
    }
}

/// Die Merkliste mit der schnellsten Route. Ohne angepinnte Geräte zeigt sie nichts.
#[component]
pub fn PinList() -> impl IntoView {
    let s = expect_context::<Store>();
    let pinned = Memo::new(move |_| s.devices.with(|d| s.favorites.with(|f| pinned_devices(d, f))));
    let (legs, url) = route_memos(s);
    let count = Memo::new(move |_| pinned.with(Vec::len));
    // Die Merkliste zeigt zuerst zehn Geräte. Die Route zählt ohnehin höchstens elf Punkte.
    let shown = RwSignal::new(10usize);
    let total = Signal::derive(move || count.get());
    let visible = move || s.on(Feature::Favorites) && count.get() > 0;
    // Am Handy zunächst zugeklappt: Die Liste kann lang werden.
    let open = RwSignal::new(!crate::jsutil::is_narrow());
    view! {
        <Show when=visible>
            <section class="pins surface" aria-label="Merkliste">
                <button
                    class="fold-head"
                    aria-expanded=move || open.get().to_string()
                    aria-label=move || if open.get() { "Merkliste zuklappen" } else { "Merkliste aufklappen" }
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    <h2>"Merkliste"</h2>
                    <span class="fold-end">
                        <span class="count-pill mono-num">{move || count.get()}</span>
                        {move || view! { <Icon name=if open.get() { "chevron-up" } else { "chevron-down" }/> }}
                    </span>
                </button>
                <div class="pins-body" hidden=move || !open.get()>
                    {move || {
                        url.get()
                            .map(|href| {
                                view! {
                                    <a class="btn btn-primary btn-lg pin-route" href=href target="_blank" rel="noopener">
                                        <Icon name="navigation"/>
                                        "Schnellste Route starten (Google Maps)"
                                    </a>
                                }
                            })
                    }}
                    <p class="muted pins-start">
                        {move || start_text(s.position.get().is_some(), s.home.get().is_some())}
                        {move || {
                            let n = legs.with(Vec::len);
                            let total = pinned.with(Vec::len);
                            if n < total { format!(" ({} ohne Position nicht dabei)", total - n) } else { String::new() }
                        }}
                        {move || (!is_exact(legs.with(Vec::len))).then_some(" Reihenfolge ist eine gute Näherung.")}
                    </p>
                    <div class="pins-actions">
                        <button class="btn btn-sm" disabled=move || s.locating.get() on:click=move |_| s.locate()>
                            <Icon name="locate"/>
                            {move || if s.locating.get() { "Suche Standort" } else { "Standort aktualisieren" }}
                        </button>
                    </div>
                    {move || s.position_note.get().map(|n| view! { <p class="form-error" role="status">{n}</p> })}
                    <ol class="pin-items">
                        {move || {
                            let order: Vec<String> = legs.with(|l| l.iter().map(|l| l.stop.device_id.clone()).collect());
                            let km: std::collections::HashMap<String, Option<f64>> =
                                legs.with(|l| l.iter().map(|l| (l.stop.device_id.clone(), l.km)).collect());
                            let mut list = pinned.get();
                            list.sort_by_key(|d| order.iter().position(|id| *id == d.id).unwrap_or(usize::MAX));
                            list.into_iter()
                                .take(shown.get())
                                .map(|d| {
                                    let id = d.id.clone();
                                    let distance = km.get(&d.id).copied().flatten();
                                    let badge_id = d.id.clone();
                                    let title = format::device_title(&d.id, d.name.as_deref());
                                    view! {
                                        <li class="pin-item">
                                            <div class="pin-main">
                                                <DeviceLink id=id.clone() class="pin-id mono-num">
                                                    {title}
                                                </DeviceLink>
                                                <StatusDot status=d.status/>
                                                <SimBadge id=badge_id/>
                                                <span class="muted mono-num">
                                                    {match (d.lat.is_some(), distance) {
                                                        (false, _) => "ohne Position".to_owned(),
                                                        (true, Some(km)) => format!("{} km", format::number(km, 1)),
                                                        (true, None) => "Start".to_owned(),
                                                    }}
                                                </span>
                                            </div>
                                            <div class="pin-actions">
                                                <SwapButton device_id=id.clone() compact=true/>
                                                <PinButton id=id/>
                                            </div>
                                        </li>
                                    }
                                })
                                .collect_view()
                        }}
                    </ol>
                    <crate::limit::MoreButton total=total shown=shown step=10/>
                    {move || {
                        (legs.with(Vec::len) > 10)
                            .then(|| view! { <p class="muted pins-start">"Google Maps nimmt höchstens elf Punkte in eine Route: Verlinkt sind die ersten elf der Reihenfolge."</p> })
                    }}
                </div>
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use shared::{DataSource, DeviceStatus};

    fn device(id: &str, pos: Option<(f64, f64)>, status: DeviceStatus) -> Device {
        Device {
            id: id.into(),
            name: Some("Nord".into()),
            status,
            soc_percent: None,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: pos.map(|p| p.0),
            lon: pos.map(|p| p.1),
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    #[test]
    fn only_existing_pinned_devices_are_listed_in_id_order() {
        let devices = [
            device("C", None, DeviceStatus::Normal),
            device("A", None, DeviceStatus::Normal),
            device("B", None, DeviceStatus::Normal),
        ];
        let pinned: BTreeSet<String> = ["C", "A", "GELOESCHT"].into_iter().map(String::from).collect();
        let ids: Vec<String> = pinned_devices(&devices, &pinned).into_iter().map(|d| d.id).collect();
        assert_eq!(ids, ["A", "C"]);
        assert!(pinned_devices(&devices, &BTreeSet::new()).is_empty());
    }

    #[test]
    fn only_pinned_devices_with_a_position_become_stops() {
        let devices = [
            device("A", Some((48.0, 9.0)), DeviceStatus::Critical),
            device("B", None, DeviceStatus::Normal),
        ];
        let stops = pinned_stops(&devices);
        assert_eq!(stops.len(), 1);
        assert_eq!(
            (stops[0].device_id.as_str(), stops[0].title.as_str()),
            ("A", "Nord (A)")
        );
        assert!(stops[0].urgent);
        assert_eq!(stops[0].reason, "Kritisch");
    }

    #[test]
    fn the_route_link_starts_at_the_user_and_visits_in_the_best_order() {
        let devices = [
            device("FAR", Some((0.0, 3.0)), DeviceStatus::Normal),
            device("NEAR", Some((0.0, 1.0)), DeviceStatus::Normal),
            device("MID", Some((0.0, 2.0)), DeviceStatus::Normal),
        ];
        let legs = ordered_legs(Some((0.0, 0.0)), pinned_stops(&devices));
        let url = maps_url(Some((0.0, 0.0)), &legs).unwrap();
        assert!(url.starts_with("https://www.google.com/maps/dir/?api=1&origin=0.000000,0.000000&destination=0.000000,3.000000&travelmode=driving"), "{url}");
        assert!(
            url.ends_with("&waypoints=0.000000,1.000000%7C0.000000,2.000000"),
            "{url}"
        );
        assert_eq!(maps_url(None, &[]), None, "ohne Halte kein Link");
    }

    #[test]
    fn the_start_is_described_in_words() {
        assert_eq!(start_text(true, true), "Start: dein Standort");
        assert_eq!(start_text(false, true), "Start: Standardstandort");
        assert_eq!(start_text(false, false), "Start: das erste Gerät der Route");
    }
}
