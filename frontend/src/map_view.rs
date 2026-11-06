//! Standortkarte mit farbigen Markern (Leaflet über JavaScript-Brücke).

use leptos::html;
use leptos::prelude::*;
use shared::DeviceStatus;

use crate::markers::{markers_json, missing_fix_text};
use crate::state::Store;
use crate::{format, jsutil};

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
