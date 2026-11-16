//! Teilen und Exportieren: Filter als Link-Fragment, die gefilterte Geräteliste als CSV und die Statistik eines Geräts
//! als aufgereihter Text zum Weiterschicken (rein, ohne Browser).

use shared::api::{DeviceSort, SortDir};
use shared::{DataSource, Device, DeviceStatus};

use crate::filter::ViewFilter;
use crate::{format, insights};

/// Eingaben für den Statistiktext. Zeiten kommen schon als Text, weil die Ortszeit nur der Browser kennt.
pub struct Report<'a> {
    pub device: &'a Device,
    pub critical_pct: f32,
    /// Wie lange das Gerät schon in seinem Zustand ist, zum Beispiel `2 Std. 14 Min.`.
    pub state_since: Option<String>,
    pub last_contact: Option<String>,
    pub installed: Option<String>,
    /// Zeitpunkt des Berichts, zum Beispiel `13.11.2026, 22:26`.
    pub taken_at: String,
}

/// Angaben im Statistiktext, die dick geschrieben werden: das, was man beim Weiterschicken zuerst lesen soll.
pub const STRONG_LABELS: [&str; 3] = ["Status", "Ladestand", "Prognose"];

/// Eine Zeile des Textes, geteilt in den normalen Anfang und den Teil, der dick geschrieben wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlighted {
    pub head: String,
    pub strong: String,
}

/// Zerlegt eine Zeile des Statistiktextes: bei der Überschrift (`Battery Bandits: TN0001`) wird der Name des Geräts
/// dick, bei Status, Ladestand und Prognose der Wert. Alle anderen Zeilen bleiben normal.
pub fn highlight(line: &str) -> Highlighted {
    if let Some((label, value)) = line.split_once(": ")
        && (label == "Battery Bandits" || STRONG_LABELS.contains(&label))
    {
        return Highlighted {
            head: format!("{label}: "),
            strong: value.to_owned(),
        };
    }
    Highlighted {
        head: line.to_owned(),
        strong: String::new(),
    }
}

/// Der Text für WhatsApp: Dort macht `*Text*` die Schrift dick. Andere Dienste zeigen Sternchen nur als Zeichen,
/// deshalb bekommen sie den Text ohne.
pub fn whatsapp_markup(text: &str) -> String {
    text.lines()
        .map(|line| {
            let h = highlight(line);
            if h.strong.is_empty() {
                h.head
            } else {
                format!("{}*{}*", h.head, h.strong)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Die Statistik eines Geräts als Text, jede Angabe in einer Zeile ("Ladestand: 12,4 %"). Das lässt sich in jedem
/// Messenger und in jeder Mail lesen, ohne dass Spalten verrutschen. Fehlende Werte stehen nicht drin.
pub fn device_report(r: &Report) -> String {
    let d = r.device;
    let mut lines = vec![
        format!("Battery Bandits: {}", format::device_title(&d.id, d.name.as_deref())),
        format!("Stand: {}", r.taken_at),
        String::new(),
    ];
    let status = match &r.state_since {
        Some(since) => format!("{} (seit {since})", format::status_label(d.status)),
        None => format::status_label(d.status).to_owned(),
    };
    lines.push(format!("Status: {status}"));
    lines.push(format!(
        "Ladestand: {}",
        d.soc_percent.map_or_else(|| "unbekannt".to_owned(), format::percent)
    ));
    if let Some(mv) = d.voltage_mv {
        lines.push(format!("Spannung: {}", format::voltage(mv)));
    }
    if let Some(ma) = d.current_ma {
        lines.push(format!("Entladestrom: {}", format::current(ma)));
    }
    if let Some(c) = d.temperature_c {
        lines.push(format!("Temperatur: {}", format::temperature(c)));
    }
    if let Some(forecast) = insights::forecast_short(d, r.critical_pct) {
        lines.push(format!("Prognose: {forecast}"));
    }
    if let Some(contact) = &r.last_contact {
        lines.push(format!("Letzter Kontakt: {contact}"));
    }
    if let Some(since) = &r.installed {
        lines.push(format!(
            "Batterie seit: {since} ({} Ah entnommen)",
            format::number(d.discharged_ah, 1)
        ));
    }
    if let (Some(lat), Some(lon)) = (d.lat, d.lon) {
        lines.push(format!(
            "Position: {}, {} (https://www.google.com/maps?q={lat:.6},{lon:.6})",
            format::coordinate(lat),
            format::coordinate(lon)
        ));
    }
    // Echte Geräte brauchen keine Herkunft: Nur Erfundenes wird gekennzeichnet.
    if d.data_source == DataSource::Simulation {
        lines.push("Herkunft: erfunden (Simulator)".to_owned());
    }
    lines.join("\n")
}

pub fn sort_key(sort: DeviceSort) -> &'static str {
    match sort {
        DeviceSort::Id => "id",
        DeviceSort::SocPercent => "soc_percent",
        DeviceSort::LastSeenAt => "last_seen_at",
        DeviceSort::Status => "status",
    }
}

pub fn parse_sort(value: &str) -> DeviceSort {
    match value {
        "soc_percent" => DeviceSort::SocPercent,
        "last_seen_at" => DeviceSort::LastSeenAt,
        "status" => DeviceSort::Status,
        _ => DeviceSort::Id,
    }
}

/// Prozent-Kodierung für Werte im Fragment (Bytes außer Buchstaben, Ziffern und `-_.~`).
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Fragment für den Link, nur Abweichungen von der Vorgabe. Leer, wenn alles Vorgabe ist.
pub fn filter_to_hash(filter: &ViewFilter) -> String {
    let default = ViewFilter::default();
    let mut parts: Vec<String> = Vec::new();
    if let Some(status) = filter.status {
        parts.push(format!("status={}", status.as_str()));
    }
    if let Some(source) = filter.source {
        parts.push(format!("source={}", source.as_str()));
    }
    if !filter.search.trim().is_empty() {
        parts.push(format!("q={}", encode(filter.search.trim())));
    }
    if filter.sort != default.sort {
        parts.push(format!("sort={}", sort_key(filter.sort)));
    }
    if filter.dir == SortDir::Desc {
        parts.push("dir=desc".to_owned());
    }
    if filter.attention_first {
        parts.push("first=1".to_owned());
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("#{}", parts.join("&"))
    }
}

/// Liest ein Fragment (mit oder ohne `#`). Unbekanntes wird ignoriert.
pub fn filter_from_hash(hash: &str) -> ViewFilter {
    let mut filter = ViewFilter::default();
    for pair in hash.trim_start_matches('#').split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        match key {
            "status" => filter.status = value.parse::<DeviceStatus>().ok(),
            "source" => filter.source = value.parse::<DataSource>().ok(),
            "q" => filter.search = decode(value),
            "sort" => filter.sort = parse_sort(value),
            "dir" => filter.dir = if value == "desc" { SortDir::Desc } else { SortDir::Asc },
            "first" => filter.attention_first = value == "1",
            _ => {}
        }
    }
    filter
}

fn field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn opt<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

pub const CSV_HEADER: &str =
    "device_id,name,source,status,soc_percent,voltage_mv,current_ma,temperature_c,lat,lon,last_seen_at";

/// Die Geräteliste als CSV (Komma, Punkt als Dezimalzeichen, Zeiten in UTC).
pub fn devices_to_csv(devices: &[Device]) -> String {
    let mut out = format!("{CSV_HEADER}\r\n");
    for d in devices {
        let row = [
            field(&d.id),
            field(d.name.as_deref().unwrap_or("")),
            d.data_source.as_str().to_owned(),
            d.status.as_str().to_owned(),
            d.soc_percent.map(|v| format!("{v:.2}")).unwrap_or_default(),
            opt(d.voltage_mv),
            opt(d.current_ma),
            opt(d.temperature_c),
            opt(d.lat),
            opt(d.lon),
            d.last_seen_at
                .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                .unwrap_or_default(),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod report_tests {
    use super::*;
    use chrono::DateTime;

    fn device() -> Device {
        Device {
            id: "TN0001".into(),
            name: Some("Kreuzung Nord".into()),
            status: DeviceStatus::Critical,
            soc_percent: Some(12.4),
            voltage_mv: Some(11_520),
            current_ma: Some(-812),
            temperature_c: Some(12.3),
            lat: Some(48.784),
            lon: Some(9.1829),
            altitude_m: None,
            satellites: None,
            last_seen_at: None,
            soc_start_percent: None,
            discharged_ah: 41.23,
            battery_installed_at: None,
            data_source: DataSource::Dozent,
            discharge_rate_pct_per_hour: Some(2.0),
            forecast_hours_remaining: Some(6.2),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        }
    }

    fn report(device: &Device) -> Report<'_> {
        Report {
            device,
            critical_pct: 20.0,
            state_since: Some("2 Std. 14 Min.".into()),
            last_contact: Some("13.11.2026, 22:20".into()),
            installed: Some("02.11.2026".into()),
            taken_at: "13.11.2026, 22:26".into(),
        }
    }

    #[test]
    fn the_report_lists_every_statistic_on_its_own_line() {
        let d = device();
        let text = device_report(&report(&d));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Battery Bandits: Kreuzung Nord (TN0001)");
        assert_eq!(lines[1], "Stand: 13.11.2026, 22:26");
        assert_eq!(lines[2], "");
        assert_eq!(lines[3], "Status: Kritisch (seit 2 Std. 14 Min.)");
        assert_eq!(lines[4], "Ladestand: 12,4 %");
        assert_eq!(lines[5], "Spannung: 11,52 V");
        assert_eq!(lines[6], "Entladestrom: 812 mA");
        assert_eq!(lines[7], "Temperatur: 12,3 °C");
        assert!(lines[8].starts_with("Prognose: "), "{}", lines[8]);
        assert_eq!(lines[9], "Letzter Kontakt: 13.11.2026, 22:20");
        assert_eq!(lines[10], "Batterie seit: 02.11.2026 (41,2 Ah entnommen)");
        assert_eq!(
            lines[11],
            "Position: 48,78400, 9,18290 (https://www.google.com/maps?q=48.784000,9.182900)"
        );
        assert_eq!(lines.len(), 12, "echte Geräte tragen keine Herkunftszeile");
    }

    #[test]
    fn missing_values_are_left_out_and_simulated_devices_say_so() {
        let d = Device {
            name: None,
            soc_percent: None,
            voltage_mv: None,
            current_ma: None,
            temperature_c: None,
            lat: None,
            lon: None,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            data_source: DataSource::Simulation,
            ..device()
        };
        let text = device_report(&Report {
            state_since: None,
            last_contact: None,
            installed: None,
            ..report(&d)
        });
        assert_eq!(
            text,
            "Battery Bandits: TN0001\nStand: 13.11.2026, 22:26\n\nStatus: Kritisch\nLadestand: unbekannt\nHerkunft: erfunden (Simulator)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    #[test]
    fn default_filter_has_no_fragment() {
        assert_eq!(filter_to_hash(&ViewFilter::default()), "");
        assert_eq!(filter_from_hash(""), ViewFilter::default());
        assert_eq!(filter_from_hash("#"), ViewFilter::default());
    }

    #[test]
    fn filter_roundtrips_through_the_fragment() {
        let filter = ViewFilter {
            status: Some(DeviceStatus::Critical),
            source: Some(DataSource::Simulation),
            search: "Haupt bahnhof & Co".into(),
            sort: DeviceSort::SocPercent,
            dir: SortDir::Desc,
            attention_first: true,
        };
        let hash = filter_to_hash(&filter);
        assert!(
            hash.starts_with('#') && !hash.contains(' ') && !hash.contains("& "),
            "{hash}"
        );
        assert_eq!(filter_from_hash(&hash), filter);
    }

    #[test]
    fn unknown_or_broken_fragments_fall_back_to_defaults() {
        assert_eq!(
            filter_from_hash("#status=quatsch&x=1&kaputt&sort=nix"),
            ViewFilter::default()
        );
    }

    #[test]
    fn percent_encoding_roundtrips_umlauts() {
        for text in ["Schlossplatz", "Müller & Söhne", "a b+c%d"] {
            assert_eq!(decode(&encode(text)), text);
        }
        assert_eq!(decode("a+b"), "a b");
        assert_eq!(decode("100%"), "100%", "unvollständige Kodierung bleibt erhalten");
    }

    #[test]
    fn csv_has_header_quotes_and_empty_cells() {
        let mut d = Device {
            id: "TN,1".into(),
            name: Some("Haupt \"Bahnhof\"".into()),
            status: DeviceStatus::Critical,
            soc_percent: Some(12.345),
            voltage_mv: Some(11_000),
            current_ma: None,
            temperature_c: None,
            lat: Some(48.5),
            lon: Some(9.0),
            altitude_m: None,
            satellites: None,
            last_seen_at: DateTime::from_timestamp(0, 0),
            soc_start_percent: None,
            discharged_ah: 0.0,
            battery_installed_at: None,
            data_source: DataSource::Simulation,
            discharge_rate_pct_per_hour: None,
            forecast_hours_remaining: None,
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            status_changed_at: None,
            retired_at: None,
            retired_reason: None,
            retired_note: None,
        };
        let csv = devices_to_csv(std::slice::from_ref(&d));
        let mut lines = csv.lines();
        assert_eq!(lines.next().unwrap(), CSV_HEADER);
        assert_eq!(
            lines.next().unwrap(),
            "\"TN,1\",\"Haupt \"\"Bahnhof\"\"\",simulation,critical,12.35,11000,,,48.5,9,1970-01-01T00:00:00Z"
        );
        d.soc_percent = None;
        d.name = None;
        assert!(
            devices_to_csv(&[d])
                .lines()
                .nth(1)
                .unwrap()
                .contains(",,simulation,critical,,")
        );
        assert_eq!(devices_to_csv(&[]), format!("{CSV_HEADER}\r\n"));
    }
}

#[cfg(test)]
mod highlight_tests {
    use super::*;

    #[test]
    fn the_important_values_are_bold_and_the_rest_stays_plain() {
        assert_eq!(
            highlight("Ladestand: 12,4 %"),
            Highlighted {
                head: "Ladestand: ".into(),
                strong: "12,4 %".into()
            }
        );
        assert_eq!(
            highlight("Status: Kritisch (seit 2 Std.)").strong,
            "Kritisch (seit 2 Std.)"
        );
        assert_eq!(highlight("Prognose: noch etwa 3 Tage").strong, "noch etwa 3 Tage");
        assert_eq!(highlight("Battery Bandits: TN0001").strong, "TN0001");
        let plain = highlight("Spannung: 11,52 V");
        assert_eq!((plain.head.as_str(), plain.strong.as_str()), ("Spannung: 11,52 V", ""));
        assert_eq!(highlight("").head, "");
    }

    #[test]
    fn whatsapp_gets_stars_around_the_important_values_only() {
        let text =
            "Battery Bandits: TN0001\nStand: 13.11.2026\n\nStatus: Kritisch\nLadestand: 12,4 %\nSpannung: 11,52 V";
        assert_eq!(
            whatsapp_markup(text),
            "Battery Bandits: *TN0001*\nStand: 13.11.2026\n\nStatus: *Kritisch*\nLadestand: *12,4 %*\nSpannung: 11,52 V"
        );
    }
}
