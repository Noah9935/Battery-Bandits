//! Dringlichkeits-Anzeige der Übersicht (rein, ohne Browser): Welche Geräte brauchen jetzt jemanden, in welcher
//! Reihenfolge, und seit wann sind sie schon in diesem Zustand?

use chrono::{DateTime, Utc};
use shared::{Alert, AlertKind, AlertStatus, Device, DeviceStatus};

use crate::format;

/// Wie dringend ein Eintrag ist. Die Reihenfolge der Varianten ist die Reihenfolge in der Anzeige.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tone {
    /// Die Batterie ist fast leer.
    Critical,
    /// Das Gerät sendet nicht mehr.
    Offline,
    /// Eine Auffälligkeit, die jemand prüfen sollte.
    Hint,
    /// Der Ladestand ist niedrig, es eilt noch nicht.
    Warning,
}

impl Tone {
    /// Kurzes Wort für die Kopfzeile einer Karte.
    pub fn word(self) -> &'static str {
        match self {
            Tone::Critical => "Kritisch",
            Tone::Offline => "Offline",
            Tone::Hint => "Auffällig",
            Tone::Warning => "Warnung",
        }
    }

    /// Klasse für die Farbe im Stylesheet.
    pub fn class(self) -> &'static str {
        match self {
            Tone::Critical => "critical",
            Tone::Offline => "offline",
            Tone::Hint | Tone::Warning => "warning",
        }
    }
}

/// Ein Eintrag der Dringlichkeits-Anzeige.
#[derive(Debug, Clone, PartialEq)]
pub struct Urgent {
    pub device_id: String,
    /// Anzeigename des Geräts, falls einer hinterlegt ist.
    pub name: Option<String>,
    pub tone: Tone,
    /// Zustand in Worten, zum Beispiel `Kritisch (unter 20 %)`.
    pub label: String,
    /// Seit wann der Zustand ununterbrochen besteht.
    pub since: Option<DateTime<Utc>>,
    /// Zusatz, zum Beispiel der Ladestand.
    pub detail: String,
    soc: Option<f32>,
}

/// Schwellwerte, die in den Texten genannt werden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    pub critical_pct: f32,
    pub warning_pct: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            critical_pct: 20.0,
            warning_pct: 40.0,
        }
    }
}

/// Seit wann ist das Gerät in seinem jetzigen Zustand?
///
/// * Offline: seit dem letzten gültigen Messwert. Das ist der Moment, ab dem wirklich nichts mehr kam.
/// * Sonst: seit dem Statuswechsel, den die Überwachung festhält. Fehlt er (ältere Daten), seit Beginn der
///   offenen Meldung.
pub fn state_since(device: &Device, alert: Option<&Alert>) -> Option<DateTime<Utc>> {
    let from_alert = alert.map(|a| a.created_at);
    match device.status {
        DeviceStatus::Offline => device.last_seen_at.or(device.status_changed_at).or(from_alert),
        _ => device.status_changed_at.or(from_alert),
    }
}

fn soc_detail(device: &Device) -> String {
    device.soc_percent.map_or_else(
        || "Ladestand unbekannt".to_owned(),
        |soc| format!("Ladestand {}", format::percent(soc)),
    )
}

/// Macht aus einem Gerät einen Eintrag, falls es Handlungsbedarf gibt.
fn entry(device: &Device, alert: Option<&Alert>, t: Thresholds) -> Option<Urgent> {
    // Eine Auffälligkeit an einem sonst gesunden Gerät zählt als Hinweis. Ist das Gerät selbst kritisch oder
    // offline, geht sein Status vor: Er ist dringender als der Hinweis.
    let hint = alert.filter(|a| !a.kind.follows_status()).map(|a| a.kind);
    let (tone, label, since, detail) = match (device.status, hint) {
        (DeviceStatus::Critical, _) => (
            Tone::Critical,
            format!("Kritisch (unter {:.0} %)", t.critical_pct),
            state_since(device, alert),
            soc_detail(device),
        ),
        (DeviceStatus::Offline, _) => {
            let fault = alert.is_some_and(|a| a.kind == AlertKind::Fault);
            (
                Tone::Offline,
                if fault { "Ausgefallen" } else { "Offline" }.to_owned(),
                state_since(device, alert),
                match device.soc_percent {
                    Some(soc) => format!("zuletzt {} geladen", format::percent(soc)),
                    None => "keine Messwerte".to_owned(),
                },
            )
        }
        (_, Some(kind)) => (
            Tone::Hint,
            match kind {
                AlertKind::Drop => "Spannungseinbruch",
                _ => "Messwerte stehen",
            }
            .to_owned(),
            alert.map(|a| a.created_at),
            soc_detail(device),
        ),
        (DeviceStatus::Warning, None) => (
            Tone::Warning,
            format!("Warnung (unter {:.0} %)", t.warning_pct),
            state_since(device, alert),
            soc_detail(device),
        ),
        (DeviceStatus::Normal, None) => return None,
    };
    Some(Urgent {
        device_id: device.id.clone(),
        name: device.name.clone().filter(|n| !n.trim().is_empty()),
        tone,
        label,
        since,
        detail,
        soc: device.soc_percent,
    })
}

/// Alle Geräte mit Handlungsbedarf: kritische zuerst (das leerste vorn), dann ausgefallene (das am längsten
/// stumme vorn), dann Auffälligkeiten, zuletzt Warnungen.
pub fn urgent(devices: &[Device], alerts: &[Alert], thresholds: Thresholds) -> Vec<Urgent> {
    let mut list: Vec<Urgent> = devices
        .iter()
        .filter_map(|d| {
            let alert = alerts
                .iter()
                .find(|a| a.device_id == d.id && a.status == AlertStatus::Open);
            entry(d, alert, thresholds)
        })
        .collect();
    list.sort_by(|a, b| {
        a.tone
            .cmp(&b.tone)
            .then_with(|| match a.tone {
                Tone::Offline => a.since.cmp(&b.since),
                _ => a.soc.unwrap_or(f32::MAX).total_cmp(&b.soc.unwrap_or(f32::MAX)),
            })
            .then_with(|| a.device_id.cmp(&b.device_id))
    });
    list
}

/// Überschrift über der Anzeige: wie viele Geräte, aufgeschlüsselt nach Art.
pub fn headline(items: &[Urgent], total_devices: usize) -> String {
    if total_devices == 0 {
        return "Noch keine Geräte".to_owned();
    }
    let count = |tone: Tone| items.iter().filter(|i| i.tone == tone).count();
    let parts: Vec<String> = [
        (count(Tone::Critical), "kritisch"),
        (count(Tone::Offline), "offline"),
        (count(Tone::Hint), "auffällig"),
        (count(Tone::Warning), "mit Warnung"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, word)| format!("{n} {word}"))
    .collect();
    if parts.is_empty() {
        format!("Alles in Ordnung, {total_devices} Geräte senden")
    } else {
        parts.join(", ")
    }
}

/// Der Text, der neben der Kennung durchläuft: Zustand und Dauer in einem Satzteil.
pub fn state_line(item: &Urgent, now_ms: f64) -> String {
    match item.since {
        Some(since) => format!(
            "{} seit {}",
            item.label,
            format::span((now_ms - since.timestamp_millis() as f64) as i64)
        ),
        None => item.label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::DataSource;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    fn device(id: &str, status: DeviceStatus, soc: Option<f32>) -> Device {
        Device {
            id: id.into(),
            name: None,
            status,
            soc_percent: soc,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            altitude_m: None,
            satellites: None,
            last_seen_at: Some(at(-600)),
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: at(-90_000),
            status_changed_at: Some(at(-300)),
        }
    }

    fn alert(device: &str, kind: AlertKind, created: i64) -> Alert {
        Alert {
            id: 1,
            device_id: device.into(),
            kind,
            status: AlertStatus::Open,
            created_at: at(created),
            closed_at: None,
        }
    }

    #[test]
    fn an_offline_device_counts_from_its_last_reading() {
        let d = device("A", DeviceStatus::Offline, Some(60.0));
        assert_eq!(state_since(&d, None), Some(at(-600)), "nicht ab dem Statuswechsel");
        let never_seen = Device {
            last_seen_at: None,
            ..d.clone()
        };
        assert_eq!(state_since(&never_seen, None), Some(at(-300)));
    }

    #[test]
    fn other_states_count_from_the_status_change_or_the_alert() {
        let d = device("A", DeviceStatus::Critical, Some(10.0));
        assert_eq!(state_since(&d, None), Some(at(-300)));
        let old = Device {
            status_changed_at: None,
            ..d
        };
        let a = alert("A", AlertKind::Critical, -1_200);
        assert_eq!(state_since(&old, Some(&a)), Some(at(-1_200)));
        assert_eq!(state_since(&old, None), None);
    }

    #[test]
    fn healthy_devices_do_not_appear() {
        let devices = [
            device("OK", DeviceStatus::Normal, Some(90.0)),
            device("WARN", DeviceStatus::Warning, Some(35.0)),
        ];
        let list = urgent(&devices, &[], Thresholds::default());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].device_id, "WARN");
        assert_eq!(list[0].label, "Warnung (unter 40 %)");
        assert_eq!(list[0].detail, "Ladestand 35,0 %");
    }

    #[test]
    fn the_order_is_critical_then_offline_then_hints_then_warnings() {
        let devices = [
            device("WARN", DeviceStatus::Warning, Some(35.0)),
            device("HINT", DeviceStatus::Normal, Some(80.0)),
            device("OFF", DeviceStatus::Offline, Some(70.0)),
            device("CRIT-FULL", DeviceStatus::Critical, Some(18.0)),
            device("CRIT-EMPTY", DeviceStatus::Critical, Some(3.0)),
            device("CRIT-UNKNOWN", DeviceStatus::Critical, None),
        ];
        let alerts = [alert("HINT", AlertKind::Drop, -100)];
        let ids: Vec<String> = urgent(&devices, &alerts, Thresholds::default())
            .into_iter()
            .map(|u| u.device_id)
            .collect();
        assert_eq!(ids, ["CRIT-EMPTY", "CRIT-FULL", "CRIT-UNKNOWN", "OFF", "HINT", "WARN"]);
    }

    #[test]
    fn among_silent_devices_the_longest_silence_comes_first() {
        let long = Device {
            last_seen_at: Some(at(-9_000)),
            ..device("LONG", DeviceStatus::Offline, None)
        };
        let short = device("SHORT", DeviceStatus::Offline, None);
        let list = urgent(&[short, long], &[], Thresholds::default());
        assert_eq!(list[0].device_id, "LONG");
        assert_eq!(list[0].detail, "keine Messwerte");
    }

    #[test]
    fn labels_name_thresholds_faults_and_hints() {
        let t = Thresholds {
            critical_pct: 25.0,
            warning_pct: 50.0,
        };
        let devices = [
            device("C", DeviceStatus::Critical, Some(10.0)),
            device("F", DeviceStatus::Offline, Some(88.0)),
            device("S", DeviceStatus::Normal, None),
            device("N", DeviceStatus::Normal, Some(90.0)),
        ];
        let alerts = [
            alert("F", AlertKind::Fault, -50),
            alert("S", AlertKind::Stagnation, -70),
            // Eine geschlossene Meldung zählt nicht.
            Alert {
                status: AlertStatus::Resolved,
                ..alert("N", AlertKind::Drop, -10)
            },
        ];
        let list = urgent(&devices, &alerts, t);
        let by_id = |id: &str| list.iter().find(|u| u.device_id == id).unwrap();
        assert_eq!(by_id("C").label, "Kritisch (unter 25 %)");
        assert_eq!(by_id("F").label, "Ausgefallen");
        assert_eq!(by_id("F").detail, "zuletzt 88,0 % geladen");
        assert_eq!(by_id("S").label, "Messwerte stehen");
        assert_eq!(by_id("S").since, Some(at(-70)), "ein Hinweis zählt ab seiner Meldung");
        assert_eq!(by_id("S").detail, "Ladestand unbekannt");
        assert!(list.iter().all(|u| u.device_id != "N"));
    }

    #[test]
    fn a_critical_device_stays_critical_even_with_a_hint() {
        let devices = [device("C", DeviceStatus::Critical, Some(5.0))];
        let alerts = [alert("C", AlertKind::Drop, -40)];
        let list = urgent(&devices, &alerts, Thresholds::default());
        assert_eq!((list[0].tone, list[0].since), (Tone::Critical, Some(at(-300))));
    }

    #[test]
    fn names_are_kept_and_blank_names_dropped() {
        let named = Device {
            name: Some("Kreuzung Nord".into()),
            ..device("A", DeviceStatus::Critical, Some(5.0))
        };
        let blank = Device {
            name: Some("  ".into()),
            ..device("B", DeviceStatus::Critical, Some(6.0))
        };
        let list = urgent(&[named, blank], &[], Thresholds::default());
        assert_eq!(list[0].name.as_deref(), Some("Kreuzung Nord"));
        assert_eq!(list[1].name, None);
    }

    #[test]
    fn the_headline_counts_by_kind() {
        let devices = [
            device("C1", DeviceStatus::Critical, Some(5.0)),
            device("C2", DeviceStatus::Critical, Some(6.0)),
            device("O", DeviceStatus::Offline, None),
            device("W", DeviceStatus::Warning, Some(30.0)),
            device("H", DeviceStatus::Normal, Some(70.0)),
        ];
        let alerts = [alert("H", AlertKind::Stagnation, -5)];
        let list = urgent(&devices, &alerts, Thresholds::default());
        assert_eq!(headline(&list, 5), "2 kritisch, 1 offline, 1 auffällig, 1 mit Warnung");
        assert_eq!(headline(&[], 12), "Alles in Ordnung, 12 Geräte senden");
        assert_eq!(headline(&[], 0), "Noch keine Geräte");
    }

    #[test]
    fn the_state_line_names_the_duration() {
        let list = urgent(&[device("O", DeviceStatus::Offline, None)], &[], Thresholds::default());
        let now = (at(0).timestamp_millis() + 134 * 60_000 - 600_000) as f64;
        assert_eq!(state_line(&list[0], now), "Offline seit 2 Std. 14 Min.");
        let unknown = Urgent {
            since: None,
            ..list[0].clone()
        };
        assert_eq!(state_line(&unknown, now), "Offline");
    }

    #[test]
    fn every_tone_has_a_short_word() {
        let words: Vec<&str> = [Tone::Critical, Tone::Offline, Tone::Hint, Tone::Warning]
            .iter()
            .map(|t| t.word())
            .collect();
        assert_eq!(words, ["Kritisch", "Offline", "Auffällig", "Warnung"]);
    }

    #[test]
    fn tones_map_to_status_colours() {
        assert_eq!(Tone::Critical.class(), "critical");
        assert_eq!(Tone::Offline.class(), "offline");
        assert_eq!(Tone::Hint.class(), "warning");
        assert_eq!(Tone::Warning.class(), "warning");
    }
}
