//! Gemeinsamer Zustand der Oberfläche und das Nachladen der Daten.

use std::collections::{BTreeSet, HashMap};

use gloo_timers::callback::{Interval, Timeout};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::api::Me;
use shared::efficiency::FleetEfficiency;
use shared::notify::{ChannelStatus, NotificationRule};
use shared::{
    Alert, AlertStatus, AnomalySettings, DashboardSummary, Device, InboxItem, MaintenanceStatus, MaintenanceTask,
    Settings,
};

use crate::api;
use crate::features::{self, Feature};
use crate::filter::ViewFilter;
use crate::jsutil;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Page {
    Overview,
    Map,
    Maintenance,
    Inbox,
    Efficiency,
    /// Optionen: erreichbar über das Profil (Zahnrad), nicht über die Navigation.
    Options,
    /// Profil: erreichbar über das Bild oben rechts.
    Profile,
    /// Kontenverwaltung für Admins: erreichbar über die Optionen.
    Accounts,
}

impl Page {
    /// Alle Seiten, in der Reihenfolge ihres Aufbaus.
    pub const ALL: [Page; 8] = [
        Page::Overview,
        Page::Map,
        Page::Maintenance,
        Page::Inbox,
        Page::Efficiency,
        Page::Options,
        Page::Profile,
        Page::Accounts,
    ];

    /// Die Seiten der Navigation, auch die Reihenfolge beim Wischen und bei den Tasten 1 bis 5.
    pub const NAV: [Page; 5] = [
        Page::Overview,
        Page::Map,
        Page::Maintenance,
        Page::Inbox,
        Page::Efficiency,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Overview => "Übersicht",
            Page::Map => "Standortkarte",
            Page::Maintenance => "Wartung",
            Page::Inbox => "Postfach",
            Page::Efficiency => "Effizienz",
            Page::Options => "Optionen",
            Page::Profile => "Profil",
            Page::Accounts => "Konten",
        }
    }

    /// Platz unter allen Seiten (für die Richtung des Wechsels).
    pub fn index(self) -> usize {
        Page::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// Platz in der Navigation, `None` für Optionen und Profil.
    pub fn nav_index(self) -> Option<usize> {
        Page::NAV.iter().position(|p| *p == self)
    }
}

/// Setzt ein Signal nur, wenn sich der Wert wirklich geändert hat. Beim Nachladen im Zwei-Sekunden-Takt sind die
/// Daten meist dieselben: Ohne diese Prüfung würde jede Ansicht bei jedem Nachladen neu rechnen.
fn put<T: PartialEq + Send + Sync + 'static>(signal: RwSignal<T>, value: T) {
    if signal.with_untracked(|old| *old != value) {
        signal.set(value);
    }
}

/// Text, der bei unterbrochener Echtzeitverbindung angezeigt wird.
pub const LIVE_LOST: &str = "Die Live-Verbindung ist unterbrochen. Die Seite verbindet sich von selbst neu.";

/// Offener Dialog (Planen, Abweisen und Bestätigen laufen alle über Rückfragen).
/// Was "Rückgängig" tut: die letzte Aktion zurücknehmen.
#[derive(Debug, Clone, PartialEq)]
pub enum UndoStep {
    /// Das Erledigen oder der Wechsel eines Einsatzes.
    Task(i64),
    /// Ein ausgemustertes Gerät wieder aufnehmen.
    Reactivate(String),
    /// Die Merkliste, wie sie vor dem Leeren war.
    RestorePins(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModalKind {
    Plan {
        device_id: String,
        alert_id: Option<i64>,
    },
    Dismiss {
        alert_id: i64,
        device_id: String,
    },
    Complete {
        task_id: i64,
        device_id: String,
    },
    /// Eine Tour löschen (oder aus der eigenen Liste nehmen), erst nach Rückfrage.
    DeleteTour {
        id: i64,
        name: String,
        mine: bool,
    },
    /// Das Erledigen eines Einsatzes zurücknehmen (Falscheintragung).
    Undo {
        task_id: i64,
        device_id: String,
    },
    Ticket {
        task_id: i64,
    },
    /// Schnellerfassung: Batteriewechsel mit einem Tipp bestätigen.
    Swap {
        device_id: String,
    },
    /// Rückfrage vor dem Eintragen bei einem Node (erklärt, was dann passiert).
    Subscribe {
        device_id: String,
    },
    /// Gerät ausmustern (Grund wählen, optional Notiz).
    Retire {
        device_id: String,
    },
}

/// Ein Eintrag im Aktivitätsprotokoll.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub at_ms: f64,
    pub text: String,
}

#[derive(Clone, Copy)]
pub struct Store {
    pub page: RwSignal<Page>,
    pub summary: RwSignal<DashboardSummary>,
    pub devices: RwSignal<Vec<Device>>,
    /// Ausgemusterte Geräte (erscheinen nicht in den normalen Listen).
    pub retired: RwSignal<Vec<Device>>,
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
    /// Die letzte Aktion, die sich zurücknehmen lässt (Text für den Knopf und was dabei zu tun ist).
    pub undo: RwSignal<Option<(String, UndoStep)>>,
    pub now_ms: RwSignal<f64>,
    /// Neue Ereignisse sind eingetroffen, die Daten müssen bald neu geladen werden.
    pub dirty: RwSignal<bool>,
    /// Zählt hoch, wenn der Verlauf des geöffneten Geräts neu geladen werden soll.
    pub history_tick: RwSignal<u32>,
    /// Eingeschaltete Zusatzfunktionen (Labor).
    pub features: RwSignal<BTreeSet<Feature>>,
    pub favorites: RwSignal<BTreeSet<String>>,
    /// Trendlinien: Ladezustand der letzten 24 Stunden je Gerät.
    pub trends: RwSignal<HashMap<String, Vec<f32>>>,
    /// Neueste zuerst, höchstens 30.
    pub activity: RwSignal<Vec<Activity>>,
    /// Eingegangene Messwerte seit dem Start dieser Sitzung.
    pub messages: RwSignal<u64>,
    pub modal: RwSignal<Option<ModalKind>>,
    pub palette_open: RwSignal<bool>,
    pub help_open: RwSignal<bool>,
    /// `auto`, `light` oder `dark`.
    pub theme_mode: RwSignal<String>,
    /// Angemeldete Person, `None` solange niemand angemeldet ist.
    pub me: RwSignal<Option<Me>>,
    /// Wurde schon geprüft, ob eine Sitzung besteht? Bis dahin zeigt die Seite weder Anmeldung noch Daten.
    pub auth_checked: RwSignal<bool>,
    /// Laden und Echtzeitverbindung laufen bereits.
    pub started: RwSignal<bool>,
    /// Alle Konten, für die Auswahl der zuständigen Person.
    pub users: RwSignal<Vec<Me>>,
    /// Der Batteriekatalog (zum Auswählen beim Batteriewechsel).
    pub battery_types: RwSignal<Vec<shared::BatteryType>>,
    /// Eigene Touren und die, die andere mit einem geteilt haben (liegen auf dem Server).
    pub tours: RwSignal<Vec<shared::Tour>>,
    /// Postfach der angemeldeten Person.
    pub inbox: RwSignal<Vec<InboxItem>>,
    pub anomaly_settings: RwSignal<Option<AnomalySettings>>,
    /// Kennung des Geräts, dessen eigene Seite (`/geraete/:id`) gerade offen ist.
    pub device_page: RwSignal<Option<String>>,
    /// Gerät, auf das die Karte beim nächsten Öffnen zoomen soll ("Auf der Karte zeigen").
    pub map_goto: RwSignal<Option<String>>,
    /// Seiten, die schon einmal geöffnet waren. Sie bleiben im Speicher, der Wechsel zurück kostet dann nichts.
    pub visited: RwSignal<BTreeSet<Page>>,
    /// Richtung des letzten Seitenwechsels: -1 nach links, 1 nach rechts, 0 ohne Bewegung.
    pub nav_dir: RwSignal<i8>,
    /// Die Echtzeitverbindung war schon einmal da und ist jetzt weg.
    pub sse_down: RwSignal<bool>,
    /// Der Hinweis zum aktuellen Problem wurde weggewischt. Das Warndreieck in der Kopfzeile bleibt.
    pub problem_hidden: RwSignal<bool>,
    /// Eigener Standort (vom Browser, nach Erlaubnis).
    pub position: RwSignal<Option<(f64, f64)>>,
    /// Hinterlegter Standardstandort, falls der Browser keinen Standort liefert.
    pub home: RwSignal<Option<(f64, f64)>>,
    pub locating: RwSignal<bool>,
    /// Warum der Standort nicht ermittelt werden konnte.
    pub position_note: RwSignal<Option<String>>,
    /// Effizienz der Flotte, geladen beim ersten Öffnen der Seite.
    pub efficiency: RwSignal<Option<FleetEfficiency>>,
    pub efficiency_error: RwSignal<Option<String>>,
    /// Weiterleitungsregel der angemeldeten Person und die auf dem Server eingerichteten Dienste.
    pub rule: RwSignal<Option<NotificationRule>>,
    pub channels: RwSignal<ChannelStatus>,
    /// Nodes, bei denen sich die angemeldete Person für Eilmeldungen eingetragen hat.
    pub subscriptions: RwSignal<BTreeSet<String>>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            page: RwSignal::new(Page::Overview),
            summary: RwSignal::new(DashboardSummary::default()),
            retired: RwSignal::new(Vec::new()),
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
            undo: RwSignal::new(None),
            now_ms: RwSignal::new(jsutil::now_ms()),
            dirty: RwSignal::new(false),
            history_tick: RwSignal::new(0),
            features: RwSignal::new(features::parse_enabled(
                &jsutil::storage_get("bb-features-off").unwrap_or_default(),
            )),
            // Die Merkliste gehört dem Konto und kommt vom Server (siehe `load_pins`).
            favorites: RwSignal::new(BTreeSet::new()),
            trends: RwSignal::new(HashMap::new()),
            activity: RwSignal::new(Vec::new()),
            messages: RwSignal::new(0),
            modal: RwSignal::new(None),
            palette_open: RwSignal::new(false),
            help_open: RwSignal::new(false),
            theme_mode: RwSignal::new(jsutil::theme_mode()),
            me: RwSignal::new(None),
            auth_checked: RwSignal::new(false),
            started: RwSignal::new(false),
            users: RwSignal::new(Vec::new()),
            battery_types: RwSignal::new(Vec::new()),
            tours: RwSignal::new(Vec::new()),
            inbox: RwSignal::new(Vec::new()),
            anomaly_settings: RwSignal::new(None),
            map_goto: RwSignal::new(None),
            device_page: RwSignal::new(crate::route::device_from_path(&jsutil::location_path())),
            visited: RwSignal::new(BTreeSet::from([Page::Overview])),
            nav_dir: RwSignal::new(0),
            sse_down: RwSignal::new(false),
            problem_hidden: RwSignal::new(false),
            position: RwSignal::new(None),
            home: RwSignal::new(
                jsutil::storage_get("bb-home")
                    .as_deref()
                    .and_then(crate::routing::parse_point),
            ),
            locating: RwSignal::new(false),
            position_note: RwSignal::new(None),
            efficiency: RwSignal::new(None),
            efficiency_error: RwSignal::new(None),
            rule: RwSignal::new(None),
            channels: RwSignal::new(ChannelStatus::default()),
            subscriptions: RwSignal::new(BTreeSet::new()),
        }
    }

    /// Das aktuelle Problem in Worten, `None` wenn alles läuft: ein Fehler beim Laden oder die unterbrochene
    /// Echtzeitverbindung. (reaktiv)
    pub fn problem(&self) -> Option<String> {
        self.error
            .get()
            .or_else(|| self.sse_down.get().then(|| LIVE_LOST.to_owned()))
    }

    /// Das Warndreieck in der Kopfzeile zeigt den Hinweis zum Problem wieder an oder blendet ihn aus.
    pub fn toggle_problem(&self) {
        self.problem_hidden.update(|hidden| *hidden = !*hidden);
    }

    /// Gerät anpinnen oder wieder lösen. Beim ersten angepinnten Gerät wird der Standort für die Route abgefragt.
    pub fn toggle_pin(&self, id: &str) {
        self.toggle_favorite(id);
        let pinned = self.favorites.with_untracked(|f| f.contains(id));
        if pinned && self.position.with_untracked(Option::is_none) {
            self.locate();
        }
    }

    /// Start der Route: der eigene Standort, sonst der hinterlegte Standardstandort. (reaktiv)
    pub fn start_point(&self) -> Option<(f64, f64)> {
        self.position.get().or_else(|| self.home.get())
    }

    /// Fragt den Standort beim Browser ab. Klappt es nicht, bleibt der Standardstandort der Start.
    pub fn locate(&self) {
        if self.locating.get_untracked() {
            return;
        }
        let s = *self;
        s.locating.set(true);
        s.position_note.set(None);
        jsutil::locate(
            move |lat, lon| {
                s.position.set(Some((lat, lon)));
                s.locating.set(false);
            },
            move |message| {
                s.position_note.set(Some(message));
                s.locating.set(false);
            },
        );
    }

    /// Speichert den Standardstandort in diesem Browser (`None` löscht ihn).
    pub fn set_home(&self, home: Option<(f64, f64)>) {
        self.home.set(home);
        jsutil::storage_set("bb-home", &home.map(crate::routing::encode_point).unwrap_or_default());
    }

    /// Lädt den Batteriekatalog neu.
    pub fn load_battery_types(&self) {
        let s = *self;
        spawn_local(async move {
            if let Ok(list) = api::battery_types().await {
                put(s.battery_types, list);
            }
        });
    }

    /// Lädt die Effizienz der Flotte.
    pub fn load_efficiency(&self) {
        let s = *self;
        spawn_local(async move {
            match api::fleet_efficiency().await {
                Ok(report) => {
                    put(s.efficiency, Some(report));
                    s.efficiency_error.set(None);
                }
                Err(e) => s.efficiency_error.set(Some(e)),
            }
        });
    }

    /// Lädt die eigene Weiterleitungsregel und welche Dienste der Server bedienen kann.
    pub fn load_forwarding(&self) {
        let s = *self;
        spawn_local(async move {
            if let Ok(rule) = api::notification_rule().await {
                put(s.rule, Some(rule));
            }
            if let Ok(channels) = api::notification_channels().await {
                put(s.channels, channels);
            }
        });
    }

    /// Lädt die angemeldete Person neu (zum Beispiel nach einer geänderten Rolle).
    pub fn refresh_me(&self) {
        let s = *self;
        spawn_local(async move {
            if let Ok(me) = api::me().await {
                put(s.me, Some(me));
            }
        });
    }

    /// Prüft beim Start, ob noch eine Sitzung besteht, und lädt dann die Daten.
    pub fn check_session(&self) {
        let s = *self;
        spawn_local(async move {
            match api::me().await {
                Ok(me) => s.sign_in(me),
                Err(message) if message == api::UNAUTHORIZED => {}
                Err(message) => s.error.set(Some(message)),
            }
            s.auth_checked.set(true);
        });
    }

    /// Nach erfolgreicher Anmeldung: Daten laden. War die Seite schon gestartet (Sitzung abgelaufen),
    /// wird sie neu geladen, damit auch die Echtzeitverbindung sauber neu beginnt.
    pub fn sign_in(&self, me: Me) {
        self.me.set(Some(me));
        if self.started.get_untracked() {
            jsutil::reload();
        } else {
            self.started.set(true);
            start(*self);
        }
    }

    /// Meldet ab und lädt die Seite neu, damit keine Daten der Sitzung im Speicher bleiben.
    pub fn sign_out(&self) {
        spawn_local(async move {
            let _ = api::logout().await;
            jsutil::reload();
        });
    }

    /// Wechselt zu einer der fünf Hauptseiten und schließt dabei die Geräteseite.
    pub fn go_page(&self, page: Page) {
        let s = *self;
        if s.page.get_untracked() == page && s.device_page.get_untracked().is_none() {
            s.selected.set(None);
            return;
        }
        // Kein Übergangseffekt, der die neue Seite verzögert: Der Wechsel passiert im selben Bild.
        let from = s.page.get_untracked().index();
        s.nav_dir.set(match page.index().cmp(&from) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
        if !s.visited.with_untracked(|v| v.contains(&page)) {
            s.visited.update(|v| {
                v.insert(page);
            });
        }
        s.selected.set(None);
        s.page.set(page);
        if s.device_page.get_untracked().is_some() {
            s.device_page.set(None);
            jsutil::push_path("/");
        }
    }

    /// Öffnet die eigene Seite eines Geräts unter `/geraete/:id`.
    /// Wechselt zur Karte und zoomt auf das Gerät.
    pub fn show_on_map(&self, id: &str) {
        self.map_goto.set(Some(id.to_owned()));
        self.modal.set(None);
        self.selected.set(None);
        if self.device_page.get_untracked().is_some() {
            self.device_page.set(None);
            crate::jsutil::push_path("/");
        }
        self.go_page(Page::Map);
    }

    pub fn open_device_page(&self, id: &str) {
        let Some(path) = crate::route::device_path(id) else {
            self.selected.set(Some(id.to_owned()));
            return;
        };
        self.selected.set(None);
        self.modal.set(None);
        self.device_page.set(Some(id.to_owned()));
        jsutil::push_path(&path);
    }

    /// Kurze Meldung unten im Bild, verschwindet nach vier Sekunden.
    /// Merkt sich die letzte Aktion, damit der Knopf "Rückgängig" sie zurücknehmen kann. Er bleibt eine halbe Minute.
    pub fn offer_undo(&self, label: impl Into<String>, step: UndoStep) {
        let entry = (label.into(), step);
        self.undo.set(Some(entry.clone()));
        let undo = self.undo;
        Timeout::new(30_000, move || {
            if undo.get_untracked().as_ref() == Some(&entry) {
                undo.set(None);
            }
        })
        .forget();
    }

    /// Nimmt die letzte Aktion zurück.
    pub fn undo_last(&self) {
        let s = *self;
        let Some((label, step)) = self.undo.get_untracked() else {
            return;
        };
        self.undo.set(None);
        spawn_local(async move {
            let result = match step {
                UndoStep::Task(id) => api::undo_task(id).await.map(|_| ()),
                UndoStep::Reactivate(id) => api::reactivate_device(&id).await.map(|_| ()),
                UndoStep::RestorePins(ids) => match api::add_pins(&ids).await {
                    Ok(list) => {
                        put(s.favorites, list.into_iter().collect());
                        Ok(())
                    }
                    Err(e) => Err(e),
                },
            };
            match result {
                Ok(()) => s.say(format!("Rückgängig gemacht: {label}")),
                Err(e) => s.say(e),
            }
            s.refresh_soon();
        });
    }

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
        self.devices
            .with(|d| d.iter().find(|d| d.id == id).cloned())
            .or_else(|| self.retired.with(|d| d.iter().find(|d| d.id == id).cloned()))
    }

    /// Gibt es erfundene Geräte? (reaktiv) Nur dann zeigt die Oberfläche Herkunftsfilter und Herkunftsmarken. Auf der
    /// echten Umgebung kommt alles vom Dozenten, dort gäbe es nichts zu unterscheiden.
    pub fn has_simulation(&self) -> bool {
        self.devices.with(|d| crate::filter::has_simulation(d))
    }

    /// Ist die Zusatzfunktion eingeschaltet? (reaktiv)
    pub fn on(&self, feature: Feature) -> bool {
        self.features.with(|set| set.contains(&feature))
    }

    pub fn set_feature(&self, feature: Feature, enabled: bool) {
        self.features.update(|set| {
            if enabled {
                set.insert(feature);
            } else {
                set.remove(&feature);
            }
        });
        self.save_features();
    }

    pub fn set_all_features(&self, enabled: bool) {
        self.features.set(if enabled {
            Feature::ALL.into_iter().collect()
        } else {
            BTreeSet::new()
        });
        self.save_features();
    }

    fn save_features(&self) {
        jsutil::storage_set(
            "bb-features-off",
            &features::serialize_disabled(&self.features.get_untracked()),
        );
    }

    /// Setzt einen Node auf die Merkliste oder nimmt ihn herunter. Die Anzeige ändert sich sofort, der Server
    /// folgt. Klappt das nicht, wird die Änderung zurückgenommen.
    pub fn toggle_favorite(&self, id: &str) {
        let s = *self;
        let id = id.to_owned();
        let now_pinned = self.favorites.with_untracked(|f| !f.contains(&id));
        self.favorites.update(|set| {
            if now_pinned {
                set.insert(id.clone());
            } else {
                set.remove(&id);
            }
        });
        spawn_local(async move {
            let result = if now_pinned {
                api::pin(&id).await
            } else {
                api::unpin(&id).await
            };
            if let Err(e) = result {
                s.favorites.update(|set| {
                    if now_pinned {
                        set.remove(&id);
                    } else {
                        set.insert(id.clone());
                    }
                });
                s.say(e);
            }
        });
    }

    /// Leert die Merkliste (die Auswahl der Nodes, die man anfahren will).
    pub fn clear_favorites(&self) {
        let s = *self;
        let before = self.favorites.get_untracked();
        self.favorites.set(BTreeSet::new());
        spawn_local(async move {
            match api::clear_pins().await {
                Ok(_) => {
                    if !before.is_empty() {
                        s.offer_undo("Merkliste geleert", UndoStep::RestorePins(before.into_iter().collect()));
                    }
                }
                Err(e) => {
                    s.favorites.set(before);
                    s.say(e);
                }
            }
        });
    }

    /// Lädt die Touren (eigene und geteilte) vom Server.
    pub fn load_tours(&self) {
        let s = *self;
        spawn_local(async move {
            if let Ok(list) = api::tours().await {
                put(s.tours, list);
            }
        });
    }

    /// Merkliste und Touren kommen vom Server. Was früher in diesem Browser lag, wandert einmal je Konto dorthin,
    /// aber nur, wenn das Konto auf dem Server noch nichts hat.
    async fn load_lists(self) {
        let s = self;
        let (pins, tours) = futures_util::join!(api::pins(), api::tours());
        if let Ok(list) = &pins {
            put(s.favorites, list.iter().cloned().collect());
        }
        if let Ok(list) = &tours {
            put(s.tours, list.clone());
        }
        let (Ok(pins), Ok(tours)) = (pins, tours) else { return };
        let Some(user) = s.me.with_untracked(|m| m.as_ref().map(|m| m.username.clone())) else {
            return;
        };
        let flag = format!("bb-lists-moved-{user}");
        if jsutil::storage_get(&flag).is_some() {
            return;
        }
        jsutil::storage_set(&flag, "1");
        if pins.is_empty() {
            let old: Vec<String> = jsutil::storage_get("bb-favorites")
                .unwrap_or_default()
                .split(',')
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .collect();
            if !old.is_empty()
                && let Ok(list) = api::add_pins(&old).await
            {
                put(s.favorites, list.into_iter().collect());
            }
        }
        if !tours.iter().any(|t| t.mine) {
            let old = crate::tour_plan::parse_legacy_tours(&jsutil::storage_get("bb-tours").unwrap_or_default());
            for tour in old {
                let new = shared::NewTour {
                    name: tour.name,
                    device_ids: tour.devices,
                    share_with: Vec::new(),
                };
                let _ = api::create_tour(&new).await;
            }
            s.load_tours();
        }
    }

    pub fn set_theme(&self, mode: &str) {
        jsutil::set_theme_mode(mode);
        self.theme_mode.set(jsutil::theme_mode());
    }

    /// Eintrag oben im Aktivitätsprotokoll (höchstens 30 bleiben).
    pub fn log_activity(&self, text: String) {
        let entry = Activity {
            at_ms: jsutil::now_ms(),
            text,
        };
        self.activity.update(|list| {
            list.insert(0, entry);
            list.truncate(30);
        });
    }

    /// Lädt neu, was sich mit jedem Messwert ändern kann: Übersicht, Geräte, Meldungen, Einsätze, Einstellungen
    /// und das Postfach. Die Anfragen laufen gleichzeitig, und nur geänderte Daten lösen ein Neuzeichnen aus.
    pub fn refresh(&self) {
        let s = *self;
        spawn_local(async move {
            let (summary, devices, alerts, tasks, settings, inbox, retired) = futures_util::join!(
                api::summary(),
                api::devices(),
                api::open_alerts(),
                api::tasks(),
                api::settings(),
                api::inbox(),
                api::devices_retired(),
            );
            if let Ok(list) = inbox {
                put(s.inbox, list);
            }
            if let Ok(list) = retired {
                put(s.retired, list);
            }
            match (summary, devices, alerts, tasks, settings) {
                (Ok(summary), Ok(devices), Ok(alerts), Ok(tasks), Ok(settings)) => {
                    put(s.summary, summary);
                    put(s.devices, devices);
                    put(s.alerts, alerts);
                    put(s.tasks, tasks);
                    put(s.settings, Some(settings));
                    put(s.error, None);
                    put(s.loaded, true);
                }
                (a, b, c, d, e) => {
                    let message = a.err().or(b.err()).or(c.err()).or(d.err()).or(e.err());
                    if message.as_deref() == Some(api::UNAUTHORIZED) {
                        // Sitzung abgelaufen: zurück zur Anmeldung.
                        s.me.set(None);
                        s.error.set(None);
                    } else {
                        s.error.set(message);
                    }
                }
            }
        });
    }

    /// Lädt, was sich selten ändert: Konten, Schwellen der Auffälligkeiten, Trendlinien (Stundenmittel) und
    /// die Weiterleitung. Einmal beim Start und danach jede Minute.
    pub fn refresh_slow(&self) {
        let s = *self;
        spawn_local(async move {
            let (users, anomalies, trends) = futures_util::join!(api::users(), api::anomaly_settings(), api::trends());
            if let Ok(list) = users {
                put(s.users, list);
            }
            if let Ok(list) = api::battery_types().await {
                put(s.battery_types, list);
            }
            if let Ok(settings) = anomalies {
                put(s.anomaly_settings, Some(settings));
            }
            if let Ok(list) = trends {
                put(s.trends, list.into_iter().map(|t| (t.device_id, t.points)).collect());
            }
        });
        self.load_forwarding();
        spawn_local(async move {
            if let Ok(list) = api::subscriptions().await {
                put(s.subscriptions, list.into_iter().collect());
            }
        });
        spawn_local(s.load_lists());
        if self.efficiency.with_untracked(Option::is_some) {
            self.load_efficiency();
        }
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
    store.refresh_slow();
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
            store.refresh_slow();
        }
    })
    .forget();
}
