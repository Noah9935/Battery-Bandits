//! Tourenplanung: In welcher Reihenfolge fährt man die Geräte an, die Hilfe brauchen?
//!
//! Das ist ein Näherungsverfahren (immer zum nächsten noch offenen Halt), keine optimale Route. Die Entfernungen
//! sind Luftlinie. Für die echte Fahrstrecke gibt es einen Link, der die Halte in Google Maps öffnet.

use leptos::prelude::*;
use shared::{Alert, AlertKind, AlertStatus, Device, MaintenanceStatus, MaintenanceTask, TaskPriority};

use crate::device::DeviceLink;
use crate::icons::Icon;
use crate::state::{ModalKind, Store};
use crate::{format, jsutil};

const EARTH_RADIUS_KM: f64 = 6371.0088;
/// Google Maps nimmt in einem Link höchstens Start, Ziel und neun Zwischenhalte.
const MAX_MAP_POINTS: usize = 11;

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub device_id: String,
    pub title: String,
    pub lat: f64,
    pub lon: f64,
    /// Warum das Gerät auf der Tour steht, zum Beispiel "Kontrolle vor Ort" oder "Ladestand kritisch".
    pub reason: String,
    pub urgent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Leg {
    pub stop: Stop,
    /// Luftlinie zum vorherigen Halt (beim ersten Halt zum Start, falls einer bekannt ist).
    pub km: Option<f64>,
}

/// Luftlinie zwischen zwei Punkten in Kilometern.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

/// Geräte mit Position, die Hilfe brauchen: wegen eines offenen Einsatzes oder einer offenen Meldung.
/// Ein Einsatz geht vor, weil er schon beschlossen ist. `only_mine` zeigt nur Geräte mit eigenen Einsätzen.
pub fn stops_from(
    devices: &[Device],
    tasks: &[MaintenanceTask],
    alerts: &[Alert],
    me: Option<&str>,
    only_mine: bool,
) -> Vec<Stop> {
    devices
        .iter()
        .filter_map(|d| {
            let (lat, lon) = (d.lat?, d.lon?);
            let task = tasks
                .iter()
                .find(|t| t.device_id == d.id && t.status == MaintenanceStatus::Planned);
            let alert = alerts
                .iter()
                .find(|a| a.device_id == d.id && a.status == AlertStatus::Open);
            if only_mine && !task.is_some_and(|t| me.is_some() && t.assignee.as_deref() == me) {
                return None;
            }
            let (reason, urgent) = match (task, alert) {
                (Some(t), _) => (t.kind.label().to_owned(), t.priority == TaskPriority::High),
                (None, Some(a)) => (a.kind.label().to_owned(), a.kind == AlertKind::Critical),
                (None, None) => return None,
            };
            Some(Stop {
                device_id: d.id.clone(),
                title: format::device_title(&d.id, d.name.as_deref()),
                lat,
                lon,
                reason,
                urgent,
            })
        })
        .collect()
}

/// Reihenfolge nach dem Nächster-Nachbar-Verfahren. Ohne Start beginnt die Tour beim dringendsten Halt.
pub fn plan_tour(start: Option<(f64, f64)>, mut stops: Vec<Stop>) -> Vec<Leg> {
    // Feste Ausgangsordnung, damit gleich weit entfernte Halte immer gleich einsortiert werden.
    stops.sort_by(|a, b| b.urgent.cmp(&a.urgent).then_with(|| a.device_id.cmp(&b.device_id)));
    let mut legs = Vec::with_capacity(stops.len());
    let mut here = start;
    if start.is_none() && !stops.is_empty() {
        let first = stops.remove(0);
        here = Some((first.lat, first.lon));
        legs.push(Leg { stop: first, km: None });
    }
    while !stops.is_empty() {
        let nearest = match here {
            Some(from) => stops
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    haversine_km(from, (a.lat, a.lon)).total_cmp(&haversine_km(from, (b.lat, b.lon)))
                })
                .map_or(0, |(i, _)| i),
            None => 0,
        };
        let stop = stops.remove(nearest);
        let km = here.map(|from| haversine_km(from, (stop.lat, stop.lon)));
        here = Some((stop.lat, stop.lon));
        legs.push(Leg { stop, km });
    }
    legs
}

pub fn total_km(legs: &[Leg]) -> f64 {
    legs.iter().filter_map(|l| l.km).sum()
}

/// Link, der die Tour in Google Maps öffnet. Bei mehr als elf Punkten werden nur die ersten verlinkt.
pub fn maps_url(start: Option<(f64, f64)>, legs: &[Leg]) -> Option<String> {
    let points: Vec<(f64, f64)> = start
        .into_iter()
        .chain(legs.iter().map(|l| (l.stop.lat, l.stop.lon)))
        .take(MAX_MAP_POINTS)
        .collect();
    let point = |p: &(f64, f64)| format!("{:.6},{:.6}", p.0, p.1);
    match points.as_slice() {
        [] => None,
        [only] => Some(format!(
            "https://www.google.com/maps/dir/?api=1&destination={}",
            point(only)
        )),
        [origin, middle @ .., destination] => {
            let mut url = format!(
                "https://www.google.com/maps/dir/?api=1&origin={}&destination={}&travelmode=driving",
                point(origin),
                point(destination)
            );
            if !middle.is_empty() {
                let via: Vec<String> = middle.iter().map(point).collect();
                url.push_str(&format!("&waypoints={}", via.join("%7C")));
            }
            Some(url)
        }
    }
}

/// Der Halt, den man als Nächstes anfährt: der erste der Tour.
pub fn active_stop(legs: &[Leg]) -> Option<&Leg> {
    legs.first()
}

#[component]
pub fn TourView() -> impl IntoView {
    let s = expect_context::<Store>();
    let start = RwSignal::new(None::<(f64, f64)>);
    let only_mine = RwSignal::new(false);
    let locating = RwSignal::new(false);
    let problem = RwSignal::new(None::<String>);

    let legs = Memo::new(move |_| {
        let me = s.me.with(|m| m.as_ref().map(|m| m.username.clone()));
        let stops = s.devices.with(|d| {
            s.tasks
                .with(|t| s.alerts.with(|a| stops_from(d, t, a, me.as_deref(), only_mine.get())))
        });
        plan_tour(start.get(), stops)
    });

    let locate = move |_| {
        locating.set(true);
        problem.set(None);
        jsutil::locate(
            move |lat, lon| {
                start.set(Some((lat, lon)));
                locating.set(false);
            },
            move |message| {
                problem.set(Some(message));
                locating.set(false);
            },
        );
    };

    view! {
        <section class="surface tour">
            <div class="surface-head">
                <h2>"Tour"</h2>
                <label class="check">
                    <input
                        type="checkbox"
                        prop:checked=move || only_mine.get()
                        on:change=move |ev| only_mine.set(event_target_checked(&ev))
                    />
                    <span>"Nur meine Einsätze"</span>
                </label>
            </div>
            <div class="surface-body">
                <p class="muted">
                    "Alle Geräte mit offenem Einsatz oder offener Meldung und bekannter Position, immer zum nächstgelegenen Halt. Die Strecken sind Luftlinie."
                </p>
                <div class="tour-bar">
                    {move || {
                        if start.get().is_some() {
                            view! {
                                <span class="muted">"Start: dein Standort"</span>
                                <button class="btn" on:click=move |_| start.set(None)>"Start zurücksetzen"</button>
                            }
                                .into_any()
                        } else {
                            view! {
                                <span class="muted">"Start: dringendster Halt"</span>
                                <button class="btn" disabled=move || locating.get() on:click=locate>
                                    {move || if locating.get() { "Standort wird gesucht ..." } else { "Meinen Standort als Start nutzen" }}
                                </button>
                            }
                                .into_any()
                        }
                    }}
                </div>
                {move || problem.get().map(|p| view! { <p class="form-error" role="alert">{p}</p> })}
                {move || {
                    let list = legs.get();
                    let Some(active) = active_stop(&list).cloned() else {
                        return view! { <p class="muted">"Keine Halte: Kein Gerät mit Position hat gerade Handlungsbedarf."</p> }
                            .into_any();
                    };
                    let origin = start.get();
                    let all_url = maps_url(origin, &list);
                    let active_url = maps_url(origin, std::slice::from_ref(&active));
                    let truncated = origin.iter().count() + list.len() > MAX_MAP_POINTS;
                    let summary = format!(
                        "{} {}, zusammen etwa {} km Luftlinie.",
                        list.len(),
                        if list.len() == 1 { "Halt" } else { "Halte" },
                        format::number(total_km(&list), 1)
                    );
                    let battery_task = s
                        .planned_task(&active.stop.device_id)
                        .filter(|t| t.kind == shared::TaskKind::Battery);
                    let stop_id = active.stop.device_id.clone();
                    let page_id = active.stop.device_id.clone();
                    view! {
                        {all_url
                            .map(|url| {
                                view! {
                                    <a class="btn btn-primary btn-lg tour-all" href=url target="_blank" rel="noopener">
                                        <Icon name="map-pin"/>
                                        "Gesamte Tour in Google Maps starten"
                                    </a>
                                }
                            })}
                        {truncated
                            .then(|| {
                                view! { <p class="muted">"Google Maps nimmt höchstens elf Punkte: Verlinkt sind die ersten."</p> }
                            })}
                        <div class="active-stop" aria-label="Aktiver Halt">
                            <span class="active-label">"Aktiver Halt"</span>
                            <div class="active-main">
                                <DeviceLink id=page_id class="active-id mono-num">{active.stop.title.clone()}</DeviceLink>
                                <span class="muted">{active.stop.reason.clone()}</span>
                                <span class="mono-num muted">
                                    {match active.km {
                                        Some(km) => format!("{} km entfernt", format::number(km, 1)),
                                        None => "Start der Tour".to_owned(),
                                    }}
                                </span>
                            </div>
                            <div class="active-actions">
                                {active_url
                                    .map(|url| {
                                        view! {
                                            <a class="btn btn-lg" href=url target="_blank" rel="noopener">
                                                <Icon name="map-pin"/>
                                                "Route in Google Maps"
                                            </a>
                                        }
                                    })}
                                {match battery_task {
                                    Some(task) => {
                                        let task_id = task.id;
                                        view! {
                                            <button
                                                class="btn btn-primary btn-lg"
                                                on:click=move |_| {
                                                    s.modal
                                                        .set(
                                                            Some(ModalKind::Complete {
                                                                task_id,
                                                                device_id: stop_id.clone(),
                                                            }),
                                                        )
                                                }
                                            >
                                                "Batterie gewechselt"
                                            </button>
                                        }
                                            .into_any()
                                    }
                                    None => {
                                        view! {
                                            <button
                                                class="btn btn-primary btn-lg"
                                                on:click=move |_| {
                                                    s.modal
                                                        .set(
                                                            Some(ModalKind::Plan {
                                                                device_id: stop_id.clone(),
                                                                alert_id: None,
                                                            }),
                                                        )
                                                }
                                            >
                                                "Einsatz planen"
                                            </button>
                                        }
                                            .into_any()
                                    }
                                }}
                            </div>
                        </div>
                        <p class="tour-sum">{summary}</p>
                        <ol class="tour-list">
                            {list
                                .into_iter()
                                .map(|leg| {
                                    let id = leg.stop.device_id.clone();
                                    view! {
                                        <li class=if leg.stop.urgent { "tour-stop urgent" } else { "tour-stop" }>
                                            <DeviceLink id=id class="mono-num">{leg.stop.title.clone()}</DeviceLink>
                                            <span class="muted">{leg.stop.reason.clone()}</span>
                                            <span class="tour-km mono-num muted">
                                                {match leg.km {
                                                    Some(km) => format!("{} km", format::number(km, 1)),
                                                    None => "Start".to_owned(),
                                                }}
                                            </span>
                                        </li>
                                    }
                                })
                                .collect_view()}
                        </ol>
                    }
                        .into_any()
                }}
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use shared::{DataSource, DeviceStatus, TaskKind};

    fn stop(id: &str, lat: f64, lon: f64, urgent: bool) -> Stop {
        Stop {
            device_id: id.into(),
            title: id.into(),
            lat,
            lon,
            reason: "x".into(),
            urgent,
        }
    }

    fn device(id: &str, pos: Option<(f64, f64)>) -> Device {
        Device {
            id: id.into(),
            name: None,
            status: DeviceStatus::Critical,
            data_source: DataSource::Dozent,
            soc_percent: None,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: pos.map(|p| p.0),
            lon: pos.map(|p| p.1),
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    fn task(device: &str, assignee: Option<&str>, priority: TaskPriority) -> MaintenanceTask {
        MaintenanceTask {
            id: 1,
            device_id: device.into(),
            alert_id: None,
            status: MaintenanceStatus::Planned,
            note: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            completed_at: None,
            kind: TaskKind::Inspection,
            priority,
            assignee: assignee.map(str::to_owned),
            assignee_name: None,
            due_date: None,
            created_by_name: None,
            completed_by_name: None,
            comment_count: 0,
        }
    }

    fn alert(device: &str, kind: AlertKind) -> Alert {
        Alert {
            id: 1,
            device_id: device.into(),
            kind,
            status: AlertStatus::Open,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            closed_at: None,
        }
    }

    #[test]
    fn haversine_matches_known_distances() {
        let stuttgart = (48.7758, 9.1829);
        let leonberg = (48.7985, 9.0147);
        let km = haversine_km(stuttgart, leonberg);
        assert!((km - 12.5).abs() < 0.6, "{km}");
        assert_eq!(haversine_km(stuttgart, stuttgart), 0.0);
        let one_degree = haversine_km((0.0, 0.0), (0.0, 1.0));
        assert!((one_degree - 111.19).abs() < 0.1, "{one_degree}");
    }

    #[test]
    fn the_tour_always_goes_to_the_nearest_stop() {
        // Auf einer Linie: Start bei 0, Halte bei 3, 1, 2 Grad östlich.
        let stops = vec![
            stop("C", 0.0, 3.0, false),
            stop("A", 0.0, 1.0, false),
            stop("B", 0.0, 2.0, false),
        ];
        let legs = plan_tour(Some((0.0, 0.0)), stops);
        let order: Vec<&str> = legs.iter().map(|l| l.stop.device_id.as_str()).collect();
        assert_eq!(order, ["A", "B", "C"]);
        assert!(legs.iter().all(|l| l.km.is_some()));
        assert!((total_km(&legs) - 3.0 * 111.19).abs() < 0.5);
    }

    #[test]
    fn without_a_start_the_most_urgent_stop_opens_the_tour() {
        let stops = vec![
            stop("A", 0.0, 0.0, false),
            stop("B", 0.0, 5.0, true),
            stop("C", 0.0, 6.0, false),
        ];
        let legs = plan_tour(None, stops);
        let order: Vec<&str> = legs.iter().map(|l| l.stop.device_id.as_str()).collect();
        assert_eq!(order, ["B", "C", "A"], "B ist dringend, danach C ist näher als A");
        assert_eq!(legs[0].km, None);
    }

    #[test]
    fn the_active_stop_is_the_first_leg() {
        let legs = plan_tour(
            Some((0.0, 0.0)),
            vec![stop("FAR", 0.0, 5.0, false), stop("NEAR", 0.0, 1.0, false)],
        );
        assert_eq!(active_stop(&legs).unwrap().stop.device_id, "NEAR");
        assert!(active_stop(&[]).is_none());
    }

    #[test]
    fn the_active_stop_has_its_own_single_stop_link() {
        let legs = plan_tour(
            Some((48.0, 9.0)),
            vec![stop("A", 48.1, 9.1, false), stop("B", 48.2, 9.2, false)],
        );
        let url = maps_url(Some((48.0, 9.0)), std::slice::from_ref(active_stop(&legs).unwrap())).unwrap();
        assert_eq!(
            url,
            "https://www.google.com/maps/dir/?api=1&origin=48.000000,9.000000&destination=48.100000,9.100000&travelmode=driving"
        );
        let without_start = maps_url(None, std::slice::from_ref(active_stop(&legs).unwrap())).unwrap();
        assert_eq!(
            without_start,
            "https://www.google.com/maps/dir/?api=1&destination=48.100000,9.100000"
        );
    }

    #[test]
    fn an_empty_tour_has_no_link_and_no_length() {
        assert!(plan_tour(None, Vec::new()).is_empty());
        assert_eq!(maps_url(None, &[]), None);
        assert_eq!(total_km(&[]), 0.0);
    }

    #[test]
    fn the_maps_link_has_origin_waypoints_and_destination() {
        let legs = plan_tour(
            Some((48.0, 9.0)),
            vec![
                stop("A", 48.1, 9.1, false),
                stop("B", 48.2, 9.2, false),
                stop("C", 48.3, 9.3, false),
            ],
        );
        let url = maps_url(Some((48.0, 9.0)), &legs).unwrap();
        assert!(url.starts_with(
            "https://www.google.com/maps/dir/?api=1&origin=48.000000,9.000000&destination=48.300000,9.300000"
        ));
        assert!(
            url.contains("&waypoints=48.100000,9.100000%7C48.200000,9.200000"),
            "{url}"
        );

        let single = maps_url(None, &legs[..1]).unwrap();
        assert_eq!(
            single,
            "https://www.google.com/maps/dir/?api=1&destination=48.100000,9.100000"
        );

        let two = maps_url(None, &legs[..2]).unwrap();
        assert!(!two.contains("waypoints"), "{two}");
    }

    #[test]
    fn the_link_is_cut_at_eleven_points() {
        let stops: Vec<Stop> = (0..15)
            .map(|i| stop(&format!("D{i:02}"), 48.0 + f64::from(i) * 0.01, 9.0, false))
            .collect();
        let legs = plan_tour(Some((48.0, 9.0)), stops);
        let url = maps_url(Some((48.0, 9.0)), &legs).unwrap();
        let via = url.split("waypoints=").nth(1).unwrap();
        assert_eq!(
            via.split("%7C").count(),
            9,
            "neun Zwischenhalte: Start und Ziel sind die übrigen zwei"
        );
    }

    #[test]
    fn stops_come_from_open_tasks_and_alerts_with_a_position() {
        let devices = [
            device("TASK", Some((1.0, 1.0))),
            device("ALERT", Some((2.0, 2.0))),
            device("NOGPS", None),
            device("QUIET", Some((3.0, 3.0))),
            device("OTHER", Some((4.0, 4.0))),
        ];
        let tasks = [
            task("TASK", Some("noah"), TaskPriority::High),
            task("NOGPS", Some("noah"), TaskPriority::Normal),
            task("OTHER", Some("fynn"), TaskPriority::Normal),
        ];
        let alerts = [alert("ALERT", AlertKind::Critical), alert("TASK", AlertKind::Offline)];

        let all = stops_from(&devices, &tasks, &alerts, Some("noah"), false);
        let ids: Vec<&str> = all.iter().map(|s| s.device_id.as_str()).collect();
        assert_eq!(
            ids,
            ["TASK", "ALERT", "OTHER"],
            "ohne Position oder ohne Anlass kein Halt"
        );
        assert_eq!(
            all[0].reason, "Kontrolle vor Ort",
            "der Einsatz zählt mehr als die Meldung"
        );
        assert!(all[0].urgent && all[1].urgent && !all[2].urgent);

        let mine = stops_from(&devices, &tasks, &alerts, Some("noah"), true);
        assert_eq!(mine.iter().map(|s| s.device_id.as_str()).collect::<Vec<_>>(), ["TASK"]);
        assert!(stops_from(&devices, &tasks, &alerts, None, true).is_empty());
    }
}
