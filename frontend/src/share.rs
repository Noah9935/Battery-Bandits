//! Teilen und Exportieren: Filter als Link-Fragment und die gefilterte Geräteliste als CSV (rein, ohne Browser).

use shared::api::{DeviceSort, SortDir};
use shared::{DataSource, Device, DeviceStatus};

use crate::filter::ViewFilter;

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
    if !filter.attention_first {
        parts.push("first=0".to_owned());
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
            "first" => filter.attention_first = value != "0",
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
            attention_first: false,
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
