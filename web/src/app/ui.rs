//! Kleine, wiederverwendbare Bausteine.

use leptos::prelude::*;
use shared::Status;

use crate::format;

/// Ampelpunkt mit Text. Die Farbe allein reicht nicht, der Text sagt es auch (Barrierefreiheit).
#[component]
pub fn StatusPill(status: Status) -> impl IntoView {
    let class = match status {
        Status::Critical => "pill crit",
        _ => "pill ok",
    };
    view! { <span class=class><span class="dot"></span>{format::status(status)}</span> }
}

/// Ladestandsbalken mit Zahl.
#[component]
pub fn ChargeBar(pct: Option<f64>, status: Status) -> impl IntoView {
    let width = pct.unwrap_or(0.0).clamp(0.0, 100.0);
    let class = match status {
        Status::Critical => "bar crit",
        _ => "bar ok",
    };
    view! {
        <div class="charge">
            <div class=class role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=width.round()>
                <span style=format!("width:{width}%")></span>
            </div>
            <b>{pct.map(format::percent).unwrap_or_else(|| "keine Daten".to_owned())}</b>
        </div>
    }
}

#[component]
pub fn Loading() -> impl IntoView {
    view! { <p class="state">"Daten werden geladen"</p> }
}

#[component]
pub fn ErrorBox(#[prop(into)] message: String) -> impl IntoView {
    view! { <div class="alert" role="alert"><b>"Das hat nicht geklappt. "</b>{message}</div> }
}

#[component]
pub fn Empty(#[prop(into)] text: String) -> impl IntoView {
    view! { <p class="state empty">{text}</p> }
}

/// Zeitpunkt als lokale Uhrzeit im deutschen Format.
pub fn date_time(ms: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    String::from(date.to_locale_string("de-DE", &wasm_bindgen::JsValue::UNDEFINED))
}

pub fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}
