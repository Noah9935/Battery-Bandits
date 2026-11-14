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
    #[wasm_bindgen(js_namespace = bbMap, js_name = setMe)]
    fn bb_map_set_me(lat: f64, lon: f64, center: bool);
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = bbPick, js_name = init)]
    fn bb_pick_init(id: &str, lat: f64, lon: f64, has: bool, on_pick: &js_sys::Function) -> bool;
    #[wasm_bindgen(js_namespace = bbPick, js_name = set)]
    fn bb_pick_set(lat: f64, lon: f64);
    #[wasm_bindgen(js_namespace = bbPick, js_name = clear)]
    fn bb_pick_clear();
    #[wasm_bindgen(js_namespace = bbPick, js_name = invalidate)]
    fn bb_pick_invalidate();
    #[wasm_bindgen(js_namespace = bbPick, js_name = destroy)]
    fn bb_pick_destroy();
}

/// Baut die kleine Karte zum Auswählen eines Ortes im Element `id` auf. `on_pick` bekommt Breite und Länge.
pub fn pick_init(id: &str, start: Option<(f64, f64)>, on_pick: impl Fn(f64, f64) + 'static) -> bool {
    let callback = Closure::<dyn Fn(f64, f64)>::new(on_pick);
    let (lat, lon) = start.unwrap_or((0.0, 0.0));
    let ok = bb_pick_init(id, lat, lon, start.is_some(), callback.as_ref().unchecked_ref());
    callback.forget();
    ok
}

/// Setzt die Markierung der Auswahlkarte und springt dorthin.
pub fn pick_set(lat: f64, lon: f64) {
    bb_pick_set(lat, lon);
}

/// Entfernt die Markierung der Auswahlkarte.
pub fn pick_clear() {
    bb_pick_clear();
}

/// Lässt die Auswahlkarte ihre Größe neu messen (nach dem Einblenden).
pub fn pick_invalidate() {
    bb_pick_invalidate();
}

/// Baut die Auswahlkarte ab.
pub fn pick_destroy() {
    bb_pick_destroy();
}

fn element_by_id(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// Ändert die Liste, ohne dass die Seite springt. Der Bereich `area_id` behält mindestens seine bisherige Höhe, damit
/// die Seite nicht schrumpft und der Browser nicht nach oben schiebt. Das Element `anchor_id` bleibt an seiner Stelle
/// auf dem Bildschirm, auch wenn sich darüber etwas ändert.
pub fn keep_in_place(anchor_id: &str, area_id: &str, change: impl FnOnce() + 'static) {
    if let Some(area) = element_by_id(area_id) {
        let height = area.get_bounding_client_rect().height();
        if height > 0.0 {
            let _ = area.set_attribute("style", &format!("min-height: {height:.0}px"));
        }
    }
    let before = element_by_id(anchor_id).map(|e| e.get_bounding_client_rect().top());
    change();
    let anchor_id = anchor_id.to_owned();
    next_frame(move || {
        let Some(anchor) = element_by_id(&anchor_id) else {
            return;
        };
        let Some(before) = before else { return };
        let delta = anchor.get_bounding_client_rect().top() - before;
        if delta.abs() >= 1.0
            && let Ok(Some(view)) = anchor.closest(".view")
        {
            view.set_scroll_top(view.scroll_top() + delta.round() as i32);
        }
    });
}

/// Scrollt das Element an den oberen Rand (unter die Kopfleiste), zum Beispiel nach dem Blättern.
pub fn scroll_to_id(id: &str) {
    let id = id.to_owned();
    next_frame(move || {
        if let Some(element) = element_by_id(&id) {
            element.scroll_into_view();
        }
    });
}

/// Führt `run` im nächsten Bild aus. So blockiert teure Arbeit (zum Beispiel der Aufbau der Karte) nicht den
/// Tipp, der sie ausgelöst hat.
pub fn next_frame(run: impl FnOnce() + 'static) {
    let run = Closure::once_into_js(run);
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(run.unchecked_ref());
    }
}

fn navigator_js() -> Option<JsValue> {
    web_sys::window().map(|w| JsValue::from(w.navigator()))
}

/// Kann der Browser das Teilen-Fenster des Betriebssystems öffnen? (Handys ja, die meisten Rechner nicht.)
pub fn can_share() -> bool {
    navigator_js()
        .and_then(|n| js_sys::Reflect::has(&n, &JsValue::from_str("share")).ok())
        .unwrap_or(false)
}

/// Öffnet das Teilen-Fenster des Betriebssystems mit Titel, Text und Link.
pub fn system_share(title: &str, text: &str, url: &str) {
    let Some(nav) = navigator_js() else { return };
    let Some(share) = js_sys::Reflect::get(&nav, &JsValue::from_str("share"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    else {
        return;
    };
    let data = js_sys::Object::new();
    for (key, value) in [("title", title), ("text", text), ("url", url)] {
        let _ = js_sys::Reflect::set(&data, &JsValue::from_str(key), &JsValue::from_str(value));
    }
    let _ = share.call1(&nav, &data);
}

/// Kopiert Text in die Zwischenablage. `false`, wenn der Browser es nicht erlaubt (nur über HTTPS und localhost).
pub fn copy_text(text: &str) -> bool {
    let Some(nav) = navigator_js() else { return false };
    let Some(clipboard) = js_sys::Reflect::get(&nav, &JsValue::from_str("clipboard"))
        .ok()
        .filter(|c| !c.is_undefined() && !c.is_null())
    else {
        return false;
    };
    js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .is_some_and(|write| write.call1(&clipboard, &JsValue::from_str(text)).is_ok())
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

/// Zeigt den eigenen Standort auf der Karte mit einer eigenen Markierung (blau, mit Ring). Mit `center` springt die
/// Karte dorthin.
pub fn map_set_me(lat: f64, lon: f64, center: bool) {
    bb_map_set_me(lat, lon, center);
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
