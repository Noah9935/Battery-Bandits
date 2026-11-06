//! Gemeinsamer Zustand der Oberfläche und das Nachladen der Daten.

use gloo_timers::callback::{Interval, Timeout};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{Alert, AlertStatus, DashboardSummary, Device, MaintenanceStatus, MaintenanceTask, Settings};

use crate::api;
use crate::filter::ViewFilter;
use crate::jsutil;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Map,
    Maintenance,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Overview, Page::Map, Page::Maintenance];

    pub fn label(self) -> &'static str {
        match self {
            Page::Overview => "Übersicht",
            Page::Map => "Standortkarte",
            Page::Maintenance => "Wartung & Optionen",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Store {
    pub page: RwSignal<Page>,
    pub summary: RwSignal<DashboardSummary>,
    pub devices: RwSignal<Vec<Device>>,
    pub alerts: RwSignal<Vec<Alert>>,
    pub tasks: RwSignal<Vec<MaintenanceTask>>,
    pub settings: RwSignal<Option<Settings>>,
    pub filter: RwSignal<ViewFilter>,
    pub selected: RwSignal<Option<String>>,
    /// Ist die Echtzeitverbindung (SSE) aktiv?
    pub live: RwSignal<bool>,
    /// Hat das erste Laden schon geklappt?
    pub loaded: RwSignal<bool>,
    /// Fehlertext, solange der Server nicht erreichbar ist.
    pub error: RwSignal<Option<String>>,
    pub toast: RwSignal<Option<String>>,
    pub now_ms: RwSignal<f64>,
    /// Neue Ereignisse sind eingetroffen, die Daten müssen bald neu geladen werden.
    pub dirty: RwSignal<bool>,
    /// Zählt hoch, wenn der Verlauf des geöffneten Geräts neu geladen werden soll.
    pub history_tick: RwSignal<u32>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            page: RwSignal::new(Page::Overview),
            summary: RwSignal::new(DashboardSummary::default()),
            devices: RwSignal::new(Vec::new()),
            alerts: RwSignal::new(Vec::new()),
            tasks: RwSignal::new(Vec::new()),
            settings: RwSignal::new(None),
            filter: RwSignal::new(ViewFilter::default()),
            selected: RwSignal::new(None),
            live: RwSignal::new(false),
            loaded: RwSignal::new(false),
            error: RwSignal::new(None),
            toast: RwSignal::new(None),
            now_ms: RwSignal::new(jsutil::now_ms()),
            dirty: RwSignal::new(false),
            history_tick: RwSignal::new(0),
        }
    }

    /// Kurze Meldung unten im Bild, verschwindet nach vier Sekunden.
    pub fn say(&self, text: impl Into<String>) {
        let text = text.into();
        self.toast.set(Some(text.clone()));
        let toast = self.toast;
        Timeout::new(4_000, move || {
            if toast.get_untracked().as_deref() == Some(text.as_str()) {
                toast.set(None);
            }
        })
        .forget();
    }

    /// Offene Meldungen mit Gerätedaten, kritische zuerst.
    pub fn open_alerts(&self) -> Vec<Alert> {
        self.alerts
            .with(|a| a.iter().filter(|a| a.status == AlertStatus::Open).cloned().collect())
    }

    /// Geplanter Einsatz für ein Gerät, falls vorhanden.
    pub fn planned_task(&self, device_id: &str) -> Option<MaintenanceTask> {
        self.tasks.with(|tasks| {
            tasks
                .iter()
                .find(|t| t.device_id == device_id && t.status == MaintenanceStatus::Planned)
                .cloned()
        })
    }

    pub fn device(&self, id: &str) -> Option<Device> {
        self.devices.with(|d| d.iter().find(|d| d.id == id).cloned())
    }

    /// Lädt alles neu (Übersicht, Geräte, Meldungen, Einsätze, Einstellungen).
    pub fn refresh(&self) {
        let s = *self;
        spawn_local(async move {
            let results = (
                api::summary().await,
                api::devices().await,
                api::open_alerts().await,
                api::tasks().await,
                api::settings().await,
            );
            match results {
                (Ok(summary), Ok(devices), Ok(alerts), Ok(tasks), Ok(settings)) => {
                    s.summary.set(summary);
                    s.devices.set(devices);
                    s.alerts.set(alerts);
                    s.tasks.set(tasks);
                    s.settings.set(Some(settings));
                    s.error.set(None);
                    s.loaded.set(true);
                }
                (a, b, c, d, e) => {
                    let message = a.err().or(b.err()).or(c.err()).or(d.err()).or(e.err());
                    s.error.set(message);
                }
            }
        });
    }

    /// Nach einem Kommando (Meldung abweisen, Wechsel planen) sofort neu laden.
    pub fn refresh_soon(&self) {
        self.refresh();
        self.history_tick.update(|t| *t += 1);
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// Erstes Laden, Echtzeitverbindung und Zeitgeber.
pub fn start(store: Store) {
    store.refresh();
    crate::sse::connect(store);

    // Alle zwei Sekunden nachladen, wenn Ereignisse eingetroffen sind (Drosselung).
    Interval::new(2_000, move || {
        if store.dirty.get_untracked() {
            store.dirty.set(false);
            store.refresh();
        }
    })
    .forget();
    // Alle 20 Sekunden: Uhr für die relativen Zeitangaben, dazu ein Sicherheitsabgleich alle 60 Sekunden.
    let ticks = std::cell::Cell::new(0u32);
    Interval::new(20_000, move || {
        store.now_ms.set(jsutil::now_ms());
        ticks.set(ticks.get() + 1);
        if ticks.get().is_multiple_of(3) {
            store.refresh();
        }
    })
    .forget();
}
