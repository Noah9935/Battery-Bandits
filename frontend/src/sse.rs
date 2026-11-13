//! Echtzeitverbindung (Server-Sent Events). Der Browser verbindet sich bei Abbrüchen selbst neu.

use leptos::prelude::*;
use shared::DomainEvent;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Event, EventSource, MessageEvent};

use crate::features::Feature;
use crate::insights::{became_critical, describe_event};
use crate::jsutil;
use crate::state::Store;

/// Gerätekennung aus der Nutzlast eines Ereignisses, `None` bei unbekanntem Inhalt.
pub fn device_of(data: &str) -> Option<String> {
    DomainEvent::from_json(data).ok().map(|e| e.device_id().to_owned())
}

pub fn connect(store: Store) {
    let Ok(source) = EventSource::new("/api/events") else {
        return;
    };

    let on_open = Closure::<dyn Fn(Event)>::new(move |_| {
        store.live.set(true);
        store.sse_down.set(false);
        // Nach einer Unterbrechung könnten Ereignisse fehlen.
        store.dirty.set(true);
    });
    source.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    on_open.forget();

    // Als Problem gilt die Unterbrechung erst, wenn die Verbindung vorher stand: Beim Aufbau ist "noch nicht
    // verbunden" der normale Zustand.
    let on_error = Closure::<dyn Fn(Event)>::new(move |_| {
        if store.live.get_untracked() {
            store.sse_down.set(true);
        }
        store.live.set(false);
    });
    source.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    on_error.forget();

    let on_message = Closure::<dyn Fn(MessageEvent)>::new(move |event: MessageEvent| {
        store.dirty.set(true);
        let selected = store.selected.get_untracked();
        let data = event.data().as_string().unwrap_or_default();
        if selected.is_some() && device_of(&data) == selected {
            store.history_tick.update(|t| *t += 1);
        }
        if let Ok(domain_event) = DomainEvent::from_json(&data) {
            if matches!(domain_event, DomainEvent::ReadingSaved { .. }) {
                store.messages.update(|n| *n += 1);
            }
            if let Some(text) = describe_event(&domain_event) {
                store.log_activity(text);
            }
            if store.features.with_untracked(|f| f.contains(&Feature::Notify))
                && let Some(device) = became_critical(&domain_event)
                && jsutil::notification_permission() == "granted"
            {
                jsutil::notify("Battery Bandits", &format!("{device} ist kritisch."));
            }
        }
    });
    source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();

    // Der Server meldet "resync", wenn Ereignisse verloren gegangen sein können.
    let on_resync = Closure::<dyn Fn(Event)>::new(move |_| {
        store.dirty.set(true);
        store.history_tick.update(|t| *t += 1);
    });
    let _ = source.add_event_listener_with_callback("resync", on_resync.as_ref().unchecked_ref());
    on_resync.forget();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_device_from_event_json() {
        let json = r#"{"type":"AlertCreated","alert_id":1,"device_id":"TN0007","kind":"critical"}"#;
        assert_eq!(device_of(json).as_deref(), Some("TN0007"));
    }

    #[test]
    fn unknown_payloads_give_none() {
        assert_eq!(device_of("{}"), None);
        assert_eq!(device_of("kein json"), None);
    }
}
