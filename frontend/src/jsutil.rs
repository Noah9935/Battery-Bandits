//! Kleine Hilfen für den Browser (Zeit, Farbschema, Leaflet-Brücke).

use chrono::{DateTime, Utc};
use shared::api::Range;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = bbMap, js_name = init)]
    fn bb_map_init(id: &str, select: &js_sys::Function) -> bool;
    #[wasm_bindgen(js_namespace = bbMap, js_name = setMarkers)]
    fn bb_map_set_markers(devices: JsValue, fit: bool);
    #[wasm_bindgen(js_namespace = bbMap, js_name = focus)]
    fn bb_map_focus(id: &str) -> bool;
    #[wasm_bindgen(js_namespace = bbMap, js_name = destroy)]
    fn bb_map_destroy();
    #[wasm_bindgen(js_namespace = bbMap, js_name = invalidate)]
    fn bb_map_invalidate();
    #[wasm_bindgen(js_namespace = bbMap, js_name = setPins)]
    fn bb_map_set_pins(ids: JsValue);
}

/// Führt `run` im nächsten Bild aus. So blockiert teure Arbeit (zum Beispiel der Aufbau der Karte) nicht den
/// Tipp, der sie ausgelöst hat.
pub fn next_frame(run: impl FnOnce() + 'static) {
    let run = Closure::once_into_js(run);
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(run.unchecked_ref());
    }
}

/// Breite des Fensters in Pixeln.
pub fn viewport_width() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

/// Liegt das Ziel einer Berührung in einem Bereich, der selbst auf Wischen reagiert (Karte, Diagramm, Regler,
/// waagerecht scrollende Leisten)? Dort darf ein Wischen nicht die Seite wechseln.
pub fn in_swipe_free_zone(target: Option<web_sys::EventTarget>) -> bool {
    target
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .and_then(|e| {
            e.closest("[data-noswipe], .leaflet-container, input, textarea, select, .chart")
                .ok()
                .flatten()
        })
        .is_some()
}

/// Ist das Farbschema "Bewegung reduzieren" im System eingeschaltet?
pub fn reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok().flatten())
        .is_some_and(|m| m.matches())
}

/// Adresse der Oberfläche ohne Pfad, zum Beispiel `https://staging.example.org`.
pub fn origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default()
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

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn storage_get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok().flatten()
}

pub fn storage_set(key: &str, value: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(key, value);
    }
}

fn set_attribute(name: &str, value: Option<&str>) {
    if let Some(element) = document_element() {
        match value {
            Some(v) => {
                let _ = element.set_attribute(name, v);
            }
            None => {
                let _ = element.remove_attribute(name);
            }
        }
    }
}

/// Gespeicherte Wahl des Farbschemas: `auto` (folgt dem System), `light` oder `dark`.
pub fn theme_mode() -> String {
    match storage_get("bb-theme").as_deref() {
        Some("light") => "light".to_owned(),
        Some("dark") => "dark".to_owned(),
        _ => "auto".to_owned(),
    }
}

pub fn set_theme_mode(mode: &str) {
    match mode {
        "light" | "dark" => {
            set_attribute("data-theme", Some(mode));
            storage_set("bb-theme", mode);
        }
        _ => {
            set_attribute("data-theme", None);
            storage_set("bb-theme", "auto");
        }
    }
}

/// Wechselt zwischen hellem und dunklem Schema (Taste T) und gibt den neuen Modus zurück.
pub fn toggle_theme() -> String {
    let mode = if is_dark() { "light" } else { "dark" };
    set_theme_mode(mode);
    mode.to_owned()
}

pub fn contrast_high() -> bool {
    storage_get("bb-contrast-v2").as_deref() == Some("high")
}

pub fn set_contrast(high: bool) {
    set_attribute("data-contrast", high.then_some("high"));
    storage_set("bb-contrast-v2", if high { "high" } else { "normal" });
}

pub fn density_compact() -> bool {
    storage_get("bb-density").as_deref() == Some("compact")
}

pub fn set_density(compact: bool) {
    set_attribute("data-density", compact.then_some("compact"));
    storage_set("bb-density", if compact { "compact" } else { "comfortable" });
}

/// Ist das Fenster schmal wie ein Handy (unter 768 Pixel)?
pub fn is_narrow() -> bool {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .is_some_and(|w| w < 768.0)
}

/// Pfad der Adresse, zum Beispiel `/geraete/TN0001`.
pub fn location_path() -> String {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_else(|| "/".to_owned())
}

/// Wechselt die Adresse, ohne die Seite neu zu laden (der Zurück-Knopf des Browsers funktioniert weiter).
pub fn push_path(path: &str) {
    if let Some(window) = web_sys::window()
        && let Ok(history) = window.history()
    {
        let _ = history.push_state_with_url(&JsValue::NULL, "", Some(path));
    }
}

/// Fragment der Adresse (mit `#`) oder leer.
pub fn location_hash() -> String {
    web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default()
}

/// Setzt das Fragment, ohne einen neuen Eintrag im Verlauf anzulegen.
pub fn replace_hash(hash: &str) {
    if let Some(window) = web_sys::window() {
        let path = window.location().pathname().unwrap_or_default();
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&format!("{path}{hash}")));
        }
    }
}

/// Fragt den Standort des Geräts ab (der Browser fragt die Person um Erlaubnis). Genau einer der beiden
/// Rückrufe wird einmal aufgerufen. Ohne HTTPS (außer localhost) verweigert der Browser die Abfrage.
pub fn locate(on_ok: impl Fn(f64, f64) + 'static, on_err: impl Fn(String) + 'static) {
    let geolocation = web_sys::window().and_then(|w| w.navigator().geolocation().ok());
    let Some(geolocation) = geolocation else {
        return on_err("Dieser Browser kann den Standort nicht ermitteln.".to_owned());
    };
    let ok = Closure::once(move |position: web_sys::Position| {
        let coords = position.coords();
        on_ok(coords.latitude(), coords.longitude());
    });
    let err = Closure::once(move |error: web_sys::PositionError| {
        on_err(
            match error.code() {
                1 => "Der Standort wurde nicht freigegeben. Bitte im Browser erlauben.",
                2 => "Der Standort ist gerade nicht verfügbar.",
                _ => "Die Standortabfrage hat zu lange gedauert.",
            }
            .to_owned(),
        );
    });
    let _ = geolocation
        .get_current_position_with_error_callback(ok.as_ref().unchecked_ref(), Some(err.as_ref().unchecked_ref()));
    ok.forget();
    err.forget();
}

/// Heutiges Datum nach der Uhr des Browsers (Ortszeit).
pub fn today() -> chrono::NaiveDate {
    let now = js_sys::Date::new_0();
    chrono::NaiveDate::from_ymd_opt(now.get_full_year() as i32, now.get_month() + 1, now.get_date()).unwrap_or_default()
}

/// Lädt die Seite neu (nach An- und Abmeldung).
pub fn reload() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

pub fn set_title(text: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        document.set_title(text);
    }
}

pub fn focus_element(id: &str) {
    if let Some(element) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        && let Ok(html) = element.dyn_into::<web_sys::HtmlElement>()
    {
        let _ = html.focus();
    }
}

/// Tippt der Mensch gerade in ein Eingabefeld? Dann dürfen einzelne Tasten nichts auslösen.
pub fn is_typing() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .is_some_and(|e| {
            matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || e.has_attribute("contenteditable")
        })
}

/// Lädt Text als Datei herunter (Blob und unsichtbarer Link).
pub fn download_text(filename: &str, text: &str, mime: &str) {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(text));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Some(document) = web_sys::window().and_then(|w| w.document())
        && let Ok(anchor) = document.create_element("a")
        && let Ok(anchor) = anchor.dyn_into::<web_sys::HtmlAnchorElement>()
    {
        anchor.set_href(&url);
        anchor.set_download(filename);
        anchor.click();
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// `granted`, `denied` oder `default` (noch nicht gefragt).
pub fn notification_permission() -> String {
    match web_sys::Notification::permission() {
        web_sys::NotificationPermission::Granted => "granted".to_owned(),
        web_sys::NotificationPermission::Denied => "denied".to_owned(),
        _ => "default".to_owned(),
    }
}

pub fn request_notification_permission() {
    let _ = web_sys::Notification::request_permission();
}

pub fn notify(title: &str, body: &str) {
    let options = web_sys::NotificationOptions::new();
    options.set_body(body);
    let _ = web_sys::Notification::new_with_options(title, &options);
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

/// Springt auf der Karte zum Gerät. `false`, wenn es dort keinen Marker hat.
pub fn map_focus(id: &str) -> bool {
    bb_map_focus(id)
}

pub fn map_destroy() {
    bb_map_destroy();
}

/// Lässt die Karte ihre Größe neu messen (nötig, wenn sie unsichtbar war).
pub fn map_invalidate() {
    bb_map_invalidate();
}

/// Hebt die angepinnten Geräte auf der Karte hervor.
pub fn map_set_pins(ids: &[String]) {
    let list = js_sys::Array::new();
    for id in ids {
        list.push(&JsValue::from_str(id));
    }
    bb_map_set_pins(list.into());
}
