//! Simulation von 50 TrafficNodes rund um Stuttgart und Leonberg. Sie senden so, wie die Nodes der Vorlesung senden:
//! Topic `traffic-nodes/<Kennung>/measurements`, JSON mit Spannung, Strom (im Entladebetrieb negativ), Temperatur und
//! GPS, 12-V-Batterie (10,5 bis 12,7 V), 80 Ah Nennkapazität. Es gibt keinen Zufall bei den Geräten selbst: Jedes der
//! 50 hat einen festen Ort, ein festes Szenario und eine feste Startladung (Tabelle `NODES`), und der Startwert
//! (`seed`) macht auch das Rauschen der Messwerte nachvollziehbar.
//!
//! Der einzige Unterschied zu echten Nodes: Die Kennung ist SIM01 bis SIM50, und der Erfassungs-Service legt sie mit der
//! Herkunft "simulation" an, die Oberfläche kennzeichnet sie als "Erfunden". Wer den Simulator durch die Nodes der
//! Vorlesung ersetzen will, richtet den Erfassungs-Service auf deren Broker (`MQTT_HOST`, `MQTT_USER`, `MQTT_PASSWORD`)
//! und lässt dieses Programm weg: Am Format ändert sich nichts.
//!
//! Szenarien: gleichmäßige Entladung, Knoten, die kritisch oder mit Warnung starten, Knoten ohne GPS-Fix, Knoten, die
//! nach einigen Runden verstummen (Offline), dazu gelegentlich fehlerhafte Nachrichten.

use shared::TelemetryPayload;
use shared::estimator::{OCV_REFERENCE_TEMP_C, OCV_TEMP_COEFF_MV_PER_K, ocv_mv, soc_from_ocv};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Normal,
    /// Startet mit fast leerer Batterie (unter 20 %).
    Critical,
    /// Sendet nie eine Position.
    NoGps,
    /// Sendet nach der angegebenen Zahl Runden nichts mehr.
    GoesOffline {
        after_rounds: u32,
    },
}

#[derive(Debug, Clone)]
pub struct FleetConfig {
    pub seed: u64,
    /// Beschleunigt die Entladung für Vorführungen. 1.0 ist Echtzeit.
    /// Der gesendete Strom wird mit diesem Faktor multipliziert, so bleibt die
    /// Stromintegration des Erfassungs-Service mit der simulierten Ladung im Einklang.
    pub speedup: f64,
    /// Anteil fehlerhafter Nachrichten (0 bis 1).
    pub invalid_rate: f64,
    pub interval_secs: f64,
    pub capacity_ah: f64,
    /// Zahl der simulierten Geräte (mindestens 1).
    pub devices: usize,
    /// Topic-Vorlage mit `{id}` als Platzhalter.
    pub topic_template: String,
}

impl Default for FleetConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            speedup: 1.0,
            invalid_rate: 0.03,
            interval_secs: 10.0,
            capacity_ah: 80.0,
            devices: NODES.len(),
            topic_template: "traffic-nodes/{id}/measurements".to_owned(),
        }
    }
}

/// Eine zu sendende Nachricht.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    pub topic: String,
    pub payload: Vec<u8>,
    /// Soll der Erfassungs-Service sie annehmen?
    pub valid: bool,
}

struct SimNode {
    id: String,
    lat: f64,
    lon: f64,
    scenario: Scenario,
    /// Wahrer Ladezustand in Prozent.
    soc: f64,
    base_current_ma: f64,
    phase: f64,
    rounds: u32,
}

/// Die 50 Nodes: (Breite, Länge, Szenario, Startladung in Prozent). Die Orte liegen in Stuttgart und der Umgebung,
/// der Name des Stadtteils steht hinter jeder Zeile.
const NODES: [(f64, f64, Scenario, f64); 50] = [
    (48.7840, 9.1829, Scenario::Normal, 96.0),   // 01 Hauptbahnhof
    (48.7784, 9.1800, Scenario::Normal, 88.0),   // 02 Schlossplatz
    (48.8000, 9.1650, Scenario::Critical, 14.0), // 03 Killesberg
    (48.7745, 9.1668, Scenario::Normal, 71.0),   // 04 Feuersee
    (48.8008, 9.0145, Scenario::Normal, 64.0),   // 05 Leonberg Marktplatz
    (48.8063, 9.0090, Scenario::GoesOffline { after_rounds: 6 }, 70.0), // 06 Leonberg Bahnhof
    (48.8010, 9.0330, Scenario::Normal, 52.0),   // 07 Leonberg Ost
    (48.8265, 9.0680, Scenario::Normal, 83.0),   // 08 Ditzingen
    (48.8095, 9.2750, Scenario::NoGps, 62.0),    // 09 Fellbach
    (48.7515, 9.1700, Scenario::Normal, 45.0),   // 10 Degerloch
    (48.7260, 9.1130, Scenario::Normal, 91.0),   // 11 Vaihingen
    (48.8050, 9.2180, Scenario::Normal, 33.0),   // 12 Bad Cannstatt
    (48.7985, 9.1850, Scenario::Normal, 78.0),   // 13 Pragsattel
    (48.7790, 9.2030, Scenario::Normal, 85.0),   // 14 Ostendplatz
    (48.7650, 9.1660, Scenario::Normal, 33.0),   // 15 Marienplatz
    (48.7770, 9.1620, Scenario::Normal, 58.0),   // 16 Stuttgart West
    (48.8040, 9.2150, Scenario::Normal, 67.0),   // 17 Wilhelmsplatz
    (48.7740, 9.2600, Scenario::Normal, 74.0),   // 18 Untertürkheim
    (48.7700, 9.2400, Scenario::Critical, 11.0), // 19 Wangen
    (48.8130, 9.1560, Scenario::Normal, 92.0),   // 20 Feuerbach
    (48.8330, 9.1700, Scenario::Normal, 49.0),   // 21 Zuffenhausen
    (48.8200, 9.1100, Scenario::Normal, 31.0),   // 22 Weilimdorf
    (48.8230, 9.2150, Scenario::Normal, 81.0),   // 23 Münster
    (48.7270, 9.1650, Scenario::Normal, 60.0),   // 24 Möhringen
    (48.7480, 9.2350, Scenario::Normal, 69.0),   // 25 Sillenbuch
    (48.7220, 9.2150, Scenario::NoGps, 72.0),    // 26 Birkach
    (48.7000, 9.1950, Scenario::Normal, 88.0),   // 27 Plieningen
    (48.7960, 9.1300, Scenario::Normal, 43.0),   // 28 Botnang
    (48.7990, 9.0620, Scenario::Normal, 77.0),   // 29 Gerlingen
    (48.8280, 9.1250, Scenario::Critical, 9.0),  // 30 Korntal
    (48.8940, 9.1950, Scenario::Normal, 94.0),   // 31 Ludwigsburg
    (48.8590, 9.1850, Scenario::Normal, 56.0),   // 32 Kornwestheim
    (48.8300, 9.3170, Scenario::GoesOffline { after_rounds: 9 }, 63.0), // 33 Waiblingen
    (48.7410, 9.3050, Scenario::Normal, 80.0),   // 34 Esslingen
    (48.7230, 9.2530, Scenario::Normal, 28.0),   // 35 Nellingen
    (48.6620, 9.2170, Scenario::Normal, 65.0),   // 36 Filderstadt
    (48.6900, 9.1650, Scenario::Normal, 52.0),   // 37 Leinfelden
    (48.6900, 9.1920, Scenario::Critical, 16.0), // 38 Echterdingen
    (48.6830, 9.0120, Scenario::Normal, 87.0),   // 39 Böblingen
    (48.7130, 9.0030, Scenario::Normal, 70.0),   // 40 Sindelfingen
    (48.7940, 9.0050, Scenario::Normal, 36.0),   // 41 Leonberg Eltingen
    (48.7650, 8.9340, Scenario::Normal, 59.0),   // 42 Renningen
    (48.8060, 8.9420, Scenario::Normal, 95.0),   // 43 Rutesheim
    (48.7380, 8.9930, Scenario::Normal, 47.0),   // 44 Magstadt
    (48.8440, 9.1590, Scenario::GoesOffline { after_rounds: 14 }, 41.0), // 45 Stammheim
    (48.8350, 9.2300, Scenario::Normal, 83.0),   // 46 Mühlhausen
    (48.8420, 9.2870, Scenario::Critical, 13.0), // 47 Schmiden
    (48.8780, 9.2700, Scenario::Normal, 76.0),   // 48 Remseck
    (48.8000, 9.0850, Scenario::Normal, 38.0),   // 49 Hirschlanden
    (48.7600, 9.1550, Scenario::GoesOffline { after_rounds: 5 }, 51.0), // 50 Heslach
];

/// So viele Nodes gibt es höchstens: die Tabelle ist der ganze Bestand.
pub const MAX_DEVICES: usize = NODES.len();

/// Kleiner deterministischer Zufallsgenerator (SplitMix64), damit keine Zusatzbibliothek nötig ist.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Gleichverteilt in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Gleichverteilt in [-1, 1).
    fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
}

pub struct Fleet {
    config: FleetConfig,
    nodes: Vec<SimNode>,
    rng: Rng,
}

impl Fleet {
    pub fn new(config: FleetConfig) -> Self {
        // Zwei getrennte Zufallsströme: einer für die festen Eigenschaften der Nodes, einer für die Messwerte. So
        // senden die ersten zwölf in der ersten Runde dasselbe, egal wie viele Nodes laufen.
        let mut setup = Rng(config.seed);
        let count = config.devices.clamp(1, MAX_DEVICES);
        let nodes = NODES
            .iter()
            .take(count)
            .enumerate()
            .map(|(i, &(lat, lon, scenario, soc))| SimNode {
                id: format!("SIM{:02}", i + 1),
                lat,
                lon,
                scenario,
                soc,
                // Mittlerer Strom eines Nodes: 180 bis 440 mA (gut 2 bis 5 Watt). In Echtzeit hält eine volle 80-Ah-Batterie
                // so 1 bis 2 Wochen, eine fast leere (10 Prozent) etwa 1 bis 2 Tage.
                base_current_ma: 180.0 + setup.unit() * 260.0,
                phase: setup.unit() * std::f64::consts::TAU,
                rounds: 0,
            })
            .collect();
        let rng = Rng(config.seed ^ 0x51D0_7A11_C0DE_5EED);
        Self { config, nodes, rng }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn node_ids(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.id.clone()).collect()
    }

    /// Eine Sendrunde: höchstens eine Nachricht je Knoten.
    pub fn tick(&mut self) -> Vec<Outgoing> {
        let cfg = self.config.clone();
        let mut out = Vec::with_capacity(self.nodes.len());
        for index in 0..self.nodes.len() {
            let (rng, node) = (&mut self.rng, &mut self.nodes[index]);
            node.rounds += 1;
            if let Scenario::GoesOffline { after_rounds } = node.scenario
                && node.rounds > after_rounds
            {
                continue;
            }
            if node.soc <= 0.0 {
                continue; // leere Batterie: der Knoten ist ausgefallen
            }

            // Die Uhr der Simulation: Sie startet mittags und läuft mit der Beschleunigung. In Echtzeit (1.0) vergeht in
            // einer Runde so viel Zeit wie in der Wirklichkeit, ein Tag dauert also wirklich 24 Stunden.
            let hour = 12.0 + f64::from(node.rounds) * cfg.interval_secs * cfg.speedup / 3600.0;
            // Tagesgang: am frühen Nachmittag am höchsten, nachts am tiefsten.
            let day = (std::f64::consts::TAU * (hour - 9.0) / 24.0).sin();
            // Tagsüber verbraucht ein Node mehr (mehr Verkehr, mehr Funk), nachts weniger.
            let current_ma = node.base_current_ma * (1.0 + 0.2 * day) * (1.0 + 0.1 * rng.signed());
            let scaled_ma = current_ma * cfg.speedup;
            node.soc =
                (node.soc - 100.0 * (scaled_ma / 1000.0) * cfg.interval_secs / 3600.0 / cfg.capacity_ah).max(0.0);

            // Außentemperatur im Tagesgang (etwa 5 bis 19 Grad), je Node ein wenig anders (Standort, Sonne, Schatten).
            let temperature_c = 12.0 + 7.0 * day + 2.0 * node.phase.cos() + 0.3 * rng.signed();
            // Die Spannung folgt der gemessenen Kennlinie einer echten Batterie (`shared::estimator`), mit dem Einfluss der
            // Temperatur und einem Spannungsabfall am Innenwiderstand (20 Milliohm) unter Last.
            let rest_mv = ocv_mv(node.soc) + OCV_TEMP_COEFF_MV_PER_K * (temperature_c - OCV_REFERENCE_TEMP_C);
            let sag_mv = current_ma / 1000.0 * 20.0;
            let voltage_mv = (rest_mv - sag_mv + 20.0 * rng.signed()).max(1_000.0);

            let has_fix = node.scenario != Scenario::NoGps;
            let payload = TelemetryPayload {
                current_ma: -(scaled_ma.round() as i32),
                voltage_mv: voltage_mv.round() as i32,
                lat: has_fix.then(|| node.lat + 0.00002 * rng.signed()),
                lon: has_fix.then(|| node.lon + 0.00002 * rng.signed()),
                altitude_m: has_fix.then(|| 250.0 + 40.0 * rng.unit()),
                satellites: has_fix.then(|| 6 + (rng.next_u64() % 6) as u8),
                time: None,
                temperature_c: Some((temperature_c as f32 * 100.0).round() / 100.0),
            };
            let topic = cfg.topic_template.replace("{id}", &node.id);

            out.push(if rng.unit() < cfg.invalid_rate {
                broken_message(rng, topic, &payload)
            } else {
                Outgoing {
                    topic,
                    payload: serde_json::to_vec(&payload).expect("Nachricht ist serialisierbar"),
                    valid: true,
                }
            });
        }
        out
    }
}

/// Eine von fünf typischen Fehlerarten.
fn broken_message(rng: &mut Rng, topic: String, good: &TelemetryPayload) -> Outgoing {
    let mut value = serde_json::to_value(good).expect("Nachricht ist serialisierbar");
    let (topic, payload) = match rng.next_u64() % 5 {
        0 => (topic, b"kein json {".to_vec()),
        1 => {
            value["voltage_mV"] = 0.into();
            (topic, serde_json::to_vec(&value).unwrap())
        }
        2 => {
            value.as_object_mut().unwrap().remove("voltage_mV");
            (topic, serde_json::to_vec(&value).unwrap())
        }
        3 => {
            value["lat"] = 48.5.into();
            value["lon"] = serde_json::Value::Null;
            (topic, serde_json::to_vec(&value).unwrap())
        }
        _ => (
            "traffic-nodes/measurements".to_owned(),
            serde_json::to_vec(&value).unwrap(),
        ),
    };
    Outgoing {
        topic,
        payload,
        valid: false,
    }
}

/// Entspricht der Startladung eines Knotens die geschätzte Ladung aus seiner ersten Spannung?
pub fn estimated_soc(voltage_mv: i32) -> f32 {
    soc_from_ocv(f64::from(voltage_mv)).0 as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::node_id_from_topic;

    fn parse(m: &Outgoing) -> TelemetryPayload {
        serde_json::from_slice(&m.payload).unwrap()
    }

    fn only_valid() -> FleetConfig {
        FleetConfig {
            invalid_rate: 0.0,
            ..FleetConfig::default()
        }
    }

    #[test]
    fn there_are_fifty_nodes_with_unique_sim_ids() {
        let fleet = Fleet::new(only_valid());
        let ids = fleet.node_ids();
        assert_eq!(ids.len(), 50);
        assert_eq!((ids[0].as_str(), ids[49].as_str()), ("SIM01", "SIM50"));
        let unique: std::collections::BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), 50);
        assert!(
            ids.iter().all(|id| id.starts_with("SIM")),
            "nie mit echten Kennungen zu verwechseln"
        );
    }

    #[test]
    fn the_device_count_is_limited_to_the_table() {
        assert_eq!(Fleet::new(many(0)).node_count(), 1);
        assert_eq!(Fleet::new(many(1_000_000)).node_count(), MAX_DEVICES);
        assert_eq!(Fleet::new(many(12)).node_count(), 12);
    }

    fn many(devices: usize) -> FleetConfig {
        FleetConfig {
            devices,
            ..only_valid()
        }
    }

    #[test]
    fn the_nodes_are_fixed_and_do_not_depend_on_how_many_run() {
        // Die erste Runde der ersten zwölf ist gleich, egal wie viele Nodes laufen.
        let (mut small, mut large) = (Fleet::new(many(12)), Fleet::new(many(50)));
        assert_eq!(small.tick()[..], large.tick()[..12]);
    }

    #[test]
    fn all_fifty_send_valid_messages_in_the_region() {
        let mut fleet = Fleet::new(many(50));
        let first = fleet.tick();
        assert_eq!(first.len(), 50);
        for m in &first {
            let p = parse(m);
            assert_eq!(p.validate(), Ok(()));
            if let (Some(lat), Some(lon)) = (p.lat, p.lon) {
                assert!(
                    (48.6..=48.95).contains(&lat) && (8.9..=9.4).contains(&lon),
                    "{lat} {lon}"
                );
            }
        }
    }

    #[test]
    fn the_mix_of_scenarios_is_realistic() {
        let cfg = many(50);
        let mut fleet = Fleet::new(cfg.clone());
        let first = fleet.tick();
        let socs: Vec<f32> = first.iter().map(|m| estimated_soc(parse(m).voltage_mv)).collect();
        let critical = socs.iter().filter(|s| **s < 20.0).count();
        let warning = socs.iter().filter(|s| (20.0..40.0).contains(*s)).count();
        assert_eq!(critical, 5, "{socs:?}");
        assert!((5..=8).contains(&warning), "{warning}");
        assert_eq!(first.iter().filter(|m| !parse(m).has_position()).count(), 2);
        // Vier Nodes verstummen nach einigen Runden: SIM06, SIM33, SIM45 und SIM50.
        let mut last = 0;
        for _ in 0..20 {
            last = fleet.tick().len();
        }
        assert_eq!(last, 46);
    }

    #[test]
    fn valid_messages_pass_the_service_validation() {
        let mut fleet = Fleet::new(only_valid());
        for _ in 0..20 {
            for m in fleet.tick() {
                assert!(m.valid);
                assert!(node_id_from_topic(&m.topic).is_some(), "{}", m.topic);
                assert_eq!(parse(&m).validate(), Ok(()));
            }
        }
    }

    #[test]
    fn same_seed_gives_identical_messages() {
        let (mut a, mut b) = (Fleet::new(FleetConfig::default()), Fleet::new(FleetConfig::default()));
        for _ in 0..10 {
            assert_eq!(a.tick(), b.tick());
        }
        let mut c = Fleet::new(FleetConfig {
            seed: 7,
            ..FleetConfig::default()
        });
        assert_ne!(Fleet::new(FleetConfig::default()).tick(), c.tick());
    }

    #[test]
    fn discharge_is_continuous_and_current_is_negative() {
        let mut fleet = Fleet::new(FleetConfig {
            speedup: 600.0,
            ..only_valid()
        });
        let first = fleet.tick();
        let mut last_voltage = parse(&first[0]).voltage_mv;
        let mut dropped = false;
        for _ in 0..50 {
            let v = parse(&fleet.tick()[0]);
            assert!(v.current_ma < 0);
            dropped |= v.voltage_mv < last_voltage;
            last_voltage = v.voltage_mv;
        }
        assert!(dropped, "Spannung muss über die Zeit sinken");
        assert!(last_voltage < parse(&first[0]).voltage_mv);
    }

    #[test]
    fn in_real_time_the_battery_drains_slowly_like_a_real_node() {
        let mut fleet = Fleet::new(only_valid());
        // Eine Stunde (360 Runden zu 10 Sekunden): ein Node entnimmt etwa 0,15 bis 0,6 Ah, das sind unter einem Prozent
        // von 80 Ah. Der Strom stammt aus den gesendeten Nachrichten, genau wie ihn der Erfassungs-Service aufsummiert.
        let mut used: std::collections::BTreeMap<String, (f64, u32)> = std::collections::BTreeMap::new();
        let mut temperatures = Vec::new();
        for _ in 0..360 {
            for message in fleet.tick() {
                let p = parse(&message);
                let entry = used.entry(message.topic.clone()).or_default();
                entry.0 += f64::from(-p.current_ma) / 1000.0 * 10.0 / 3600.0;
                entry.1 += 1;
                temperatures.extend(p.temperature_c);
            }
        }
        // Nodes, die in dieser Stunde verstummen (Szenario Offline), zählen nicht.
        let full_hour: Vec<_> = used.iter().filter(|(_, (_, n))| *n == 360).collect();
        assert!(full_hour.len() >= 40, "{} Nodes mit voller Stunde", full_hour.len());
        for (topic, (ah, _)) in full_hour {
            assert!((0.1..0.65).contains(ah), "{topic} entnahm {ah} Ah in einer Stunde");
        }
        // Die Temperatur schwankt im Tagesgang und nicht im Minutentakt: in einer Stunde nur wenige Grad.
        let (low, high) = temperatures
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(*t), hi.max(*t)));
        assert!(high - low < 9.0, "{low} bis {high} Grad in einer Stunde");
    }

    #[test]
    fn a_healthy_node_lasts_more_than_a_day_in_real_time() {
        let mut fleet = Fleet::new(only_valid());
        for _ in 0..8_640 {
            fleet.tick();
        }
        let sending: Vec<String> = fleet.tick().iter().map(|m| m.topic.clone()).collect();
        assert!(
            sending.iter().any(|t| t.contains("SIM01")),
            "SIM01 (96 Prozent) muss nach einem Tag noch senden"
        );
        assert!(sending.iter().any(|t| t.contains("SIM02")));
    }

    #[test]
    fn the_third_node_starts_critical() {
        let cfg = only_valid();
        let mut fleet = Fleet::new(cfg.clone());
        let socs: Vec<f32> = fleet
            .tick()
            .iter()
            .map(|m| estimated_soc(parse(m).voltage_mv))
            .collect();
        assert!(socs[2] < 20.0);
        assert!(socs[0] > 80.0);
    }

    #[test]
    fn two_nodes_never_send_a_position() {
        let mut fleet = Fleet::new(only_valid());
        for _ in 0..5 {
            let round = fleet.tick();
            let without: Vec<_> = round.iter().filter(|m| !parse(m).has_position()).collect();
            assert_eq!(without.len(), 2);
            assert!(without[0].topic.contains("SIM09") && without[1].topic.contains("SIM26"));
        }
    }

    #[test]
    fn one_node_goes_offline() {
        let mut fleet = Fleet::new(only_valid());
        for round in 1..=10 {
            let sent = fleet.tick();
            let present = sent.iter().any(|m| m.topic.contains("SIM06"));
            assert_eq!(present, round <= 6, "Runde {round}");
        }
    }

    #[test]
    fn invalid_rate_one_makes_every_message_invalid() {
        let mut fleet = Fleet::new(FleetConfig {
            invalid_rate: 1.0,
            ..FleetConfig::default()
        });
        let round = fleet.tick();
        assert!(!round.is_empty());
        assert!(round.iter().all(|m| !m.valid));
    }

    #[test]
    fn invalid_messages_are_really_rejected_by_validation() {
        let mut fleet = Fleet::new(FleetConfig {
            invalid_rate: 1.0,
            ..FleetConfig::default()
        });
        for _ in 0..30 {
            for m in fleet.tick() {
                let parsed = serde_json::from_slice::<TelemetryPayload>(&m.payload);
                let rejected =
                    node_id_from_topic(&m.topic).is_none() || parsed.map(|p| p.validate().is_err()).unwrap_or(true);
                assert!(rejected, "{m:?}");
            }
        }
    }

    #[test]
    fn empty_battery_stops_sending() {
        let mut fleet = Fleet::new(FleetConfig {
            speedup: 100_000.0,
            ..only_valid()
        });
        let mut counts = Vec::new();
        for _ in 0..5 {
            counts.push(fleet.tick().len());
        }
        assert!(counts.last().unwrap() < counts.first().unwrap());
    }

    #[test]
    fn topic_template_is_configurable() {
        let mut fleet = Fleet::new(FleetConfig {
            topic_template: "trafficnode/{id}/telemetry".into(),
            ..only_valid()
        });
        assert_eq!(fleet.tick()[0].topic, "trafficnode/SIM01/telemetry");
    }
}
