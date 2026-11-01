//! Live-Updates per Server-Sent Events (NF01).
//!
//! Der Server schickt bei jedem Ereignis eine kleine Nachricht. Die App lädt
//! daraufhin die sichtbaren Daten neu. Bei vielen Ereignissen kurz hintereinander
//! wird höchstens einmal pro Sekunde neu geladen.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use futures_util::StreamExt;
use gloo_net::eventsource::futures::EventSource;
use gloo_timers::future::sleep;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Zähler, der bei jedem Ereignis hochgezählt wird. Wer ihn liest, lädt neu.
#[derive(Clone, Copy)]
pub struct Live {
    pub tick: ReadSignal<u32>,
    pub connected: ReadSignal<bool>,
}

/// Verbindet mit dem Ereignisstrom und gibt den Zähler zurück.
pub fn connect() -> Live {
    let (tick, set_tick) = signal(0u32);
    let (connected, set_connected) = signal(false);
    let dirty = Rc::new(Cell::new(false));

    // Sammelt Ereignisse und gibt sie gebündelt weiter.
    spawn_local({
        let dirty = dirty.clone();
        async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                if dirty.replace(false) {
                    set_tick.update(|n| *n = n.wrapping_add(1));
                }
            }
        }
    });

    // Als Rückfall alle 30 Sekunden neu laden, falls die Verbindung still ausfällt.
    spawn_local(async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            set_tick.update(|n| *n = n.wrapping_add(1));
        }
    });

    spawn_local(async move {
        loop {
            if let Ok(mut source) = EventSource::new("/api/events")
                && let Ok(mut stream) = source.subscribe("message")
            {
                set_connected.set(true);
                // Nach dem Verbinden einmal neu laden: In der Zwischenzeit kann etwas passiert sein.
                dirty.set(true);
                while stream.next().await.is_some() {
                    dirty.set(true);
                }
            }
            set_connected.set(false);
            sleep(Duration::from_secs(3)).await;
        }
    });

    Live { tick, connected }
}
