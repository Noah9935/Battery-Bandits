//! Kleine Hilfen für den Browser (Zeit, Farbschema, Leaflet-Brücke).

use chrono::{DateTime, Utc};
use shared::api::Range;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = bbMap, js_name = init)]
    fn bb_map_init(id: &str, select: &js_sys::Function) -> bool;
    #[wasm_bindgen(js_namespace = bbMap, js_name = setMarkers)]
    fn bb_map_set_markers(devices: JsValue, fit: bool);
    #[wasm_bindgen(js_namespace = bbMap, js_name = destroy)]
    fn bb_map_destroy();
}

/// Aktuelle Zeit in Millisekunden seit 1970.
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn local_date(ms: f64) -> js_sys::Date {
    js_sys::Date::new(&JsValue::from_f64(ms))
}

/// Beschriftung der Zeitachse in Ortszeit: bei 24 Stunden die Uhrzeit, sonst das Datum.
pub fn axis_label(ms: i64, range: Range) -> String {
    let d = local_date(ms as f64);
    match range {
        Range::Day => format!("{:02}:{:02}", d.get_hours(), d.get_minutes()),
        Range::Week | Range::Month => format!("{:02}.{:02}.", d.get_date(), d.get_month() + 1),
    }
}

/// `06.11.2026, 12:30` in Ortszeit.
pub fn date_time_label(time: DateTime<Utc>) -> String {
    let d = local_date(time.timestamp_millis() as f64);
    format!(
        "{:02}.{:02}.{}, {:02}:{:02}",
        d.get_date(),
        d.get_month() + 1,
        d.get_full_year(),
        d.get_hours(),
        d.get_minutes()
    )
}

/// `06.11.2026` in Ortszeit.
pub fn date_label(time: DateTime<Utc>) -> String {
    let d = local_date(time.timestamp_millis() as f64);
    format!("{:02}.{:02}.{}", d.get_date(), d.get_month() + 1, d.get_full_year())
}

fn document_element() -> Option<web_sys::HtmlElement> {
    web_sys::window()?.document()?.document_element()?.dyn_into().ok()
}

/// Ist gerade das dunkle Farbschema aktiv?
pub fn is_dark() -> bool {
    if let Some(theme) = document_element().and_then(|e| e.dataset().get("theme")) {
        return theme == "dark";
    }
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .is_some_and(|m| m.matches())
}

/// Wechselt zwischen hellem und dunklem Schema und merkt es sich im Browser.
pub fn toggle_theme() -> bool {
    let dark = !is_dark();
    let value = if dark { "dark" } else { "light" };
    if let Some(element) = document_element() {
        let _ = element.dataset().set("theme", value);
    }
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item("bb-theme", value);
    }
    dark
}

/// Baut die Karte im Element `id` auf. `on_select` wird mit der Gerätekennung aufgerufen.
pub fn map_init(id: &str, on_select: impl Fn(String) + 'static) -> bool {
    let callback = Closure::<dyn Fn(String)>::new(on_select);
    let ok = bb_map_init(id, callback.as_ref().unchecked_ref());
    callback.forget();
    ok
}

/// Setzt die Marker der Karte. `json` ist eine Liste von `{id,label,status,lat,lon}`.
pub fn map_set_markers(json: &str, fit: bool) {
    if let Ok(value) = js_sys::JSON::parse(json) {
        bb_map_set_markers(value, fit);
    }
}

pub fn map_destroy() {
    bb_map_destroy();
}
