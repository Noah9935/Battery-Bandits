//! Standortkarte mit farbigen Markern (Leaflet über JavaScript-Brücke).

use leptos::html;
use leptos::prelude::*;
use serde::Serialize;
use shared::{Device, DeviceStatus};

use crate::state::Store;
use crate::{format, jsutil};

#[derive(Serialize)]
struct Marker {
    id: String,
    label: String,
    status: DeviceStatus,
    lat: f64,
    lon: f64,
}

/// Marker für alle Geräte mit bekannter Position als JSON für die Karte.
pub fn markers_json(devices: &[Device]) -> String {
    let markers: Vec<Marker> = devices
        .iter()
        .filter_map(|d| {
            Some(Marker {
                id: d.id.clone(),
                label: format!(
                    "{}: {}",
                    format::device_title(&d.id, d.name.as_deref()),
                    d.soc_percent
                        .map_or_else(|| "Ladestand unbekannt".to_owned(), format::percent)
                ),
                status: d.status,
                lat: d.lat?,
                lon: d.lon?,
            })
        })
        .collect();
    serde_json::to_string(&markers).unwrap_or_else(|_| "[]".to_owned())
}

/// Hinweis zu Geräten ohne Position, mit richtigem Singular.
pub fn missing_fix_text(count: usize) -> String {
    if count == 1 {
        "1 Gerät ohne GPS-Fix ist nicht auf der Karte.".to_owned()
    } else {
        format!("{count} Geräte ohne GPS-Fix sind nicht auf der Karte.")
    }
}

#[component]
pub fn MapView() -> impl IntoView {
    let s = expect_context::<Store>();
    let node = NodeRef::<html::Div>::new();
    let ready = RwSignal::new(false);
    let fitted = StoredValue::new(false);

    node.on_load(move |_| {
        let ok = jsutil::map_init("bb-map", move |id| s.selected.set(Some(id)));
        ready.set(ok);
    });
    on_cleanup(jsutil::map_destroy);

    // Marker bei jeder Änderung der Geräte neu setzen. Der Ausschnitt wird nur beim ersten Mal angepasst.
    Effect::new(move |_| {
        if !ready.get() {
            return;
        }
        let json = s.devices.with(|d| markers_json(d));
        let fit = !fitted.get_value() && json != "[]";
        if fit {
            fitted.set_value(true);
        }
        jsutil::map_set_markers(&json, fit);
    });

    let without_fix = move || {
        s.devices
            .with(|d| d.iter().filter(|d| d.lat.is_none() || d.lon.is_none()).count())
    };
    view! {
        <header class="page-head">
            <h1>"Standortkarte"</h1>
            <p class="sub">"Position der TrafficNodes. Ein Klick auf einen Punkt öffnet die Details."</p>
        </header>
        <div class="map-legend">
            {DeviceStatus::ALL
                .iter()
                .map(|st| view! { <span class="legend-item"><span class=format!("swatch {}", st.as_str())></span>{format::status_label(*st)}</span> })
                .collect_view()}
            {move || {
                let n = without_fix();
                (n > 0).then(|| view! { <span class="muted">{missing_fix_text(n)}</span> })
            }}
        </div>
        <div id="bb-map" class="map" node_ref=node role="region" aria-label="Karte der TrafficNodes"></div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn device(id: &str, position: Option<(f64, f64)>) -> Device {
        Device {
            id: id.into(),
            name: Some("Hauptbahnhof".into()),
            status: DeviceStatus::Critical,
            soc_percent: Some(12.34),
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: position.map(|p| p.0),
            lon: position.map(|p| p.1),
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn only_devices_with_a_position_become_markers() {
        let json = markers_json(&[device("TN0001", Some((48.78, 9.18))), device("TN0002", None)]);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let list = value.as_array().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], "TN0001");
        assert_eq!(list[0]["status"], "critical");
        assert_eq!(list[0]["lat"], 48.78);
        assert_eq!(list[0]["label"], "Hauptbahnhof (TN0001): 12,3 %");
    }

    #[test]
    fn missing_fix_text_has_correct_grammar() {
        assert_eq!(missing_fix_text(1), "1 Gerät ohne GPS-Fix ist nicht auf der Karte.");
        assert_eq!(missing_fix_text(3), "3 Geräte ohne GPS-Fix sind nicht auf der Karte.");
    }

    #[test]
    fn no_devices_give_an_empty_list() {
        assert_eq!(markers_json(&[]), "[]");
    }
}
