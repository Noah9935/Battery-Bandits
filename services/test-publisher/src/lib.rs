//! Simulation von 12 TrafficNodes rund um Stuttgart und Leonberg.
//!
//! Alle Geräte sind erfunden und heißen SIM01 bis SIM12, damit sie nie mit echten Kennungen
//! (TN0001, 101) verwechselt werden. Nachrichten haben das echte Format von Vorlesung und Server: Topic
//! `traffic-nodes/<SIMxx>/measurements`, Strom im Entladebetrieb negativ,
//! Spannung einer 12-V-Batterie (10,5 bis 12,7 V), 80 Ah Nennkapazität.
//!
//! Szenarien: gleichmäßige Entladung, ein Knoten startet kritisch, einer ohne
//! GPS-Fix, einer verstummt nach einigen Runden (Offline), dazu gelegentlich
//! fehlerhafte Nachrichten. Alles ist deterministisch über den Startwert (`seed`).

use shared::TelemetryPayload;
use shared::soc::calculate_soc;

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
    pub min_mv: i32,
    pub max_mv: i32,
    pub capacity_ah: f64,
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
            min_mv: 10_500,
            max_mv: 12_700,
            capacity_ah: 80.0,
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

/// (Name, Breite, Länge, Szenario, Startladung in Prozent)
const NODES: [(f64, f64, Scenario, f64); 12] = [
    (48.7840, 9.1829, Scenario::Normal, 96.0),   // Hauptbahnhof
    (48.7784, 9.1800, Scenario::Normal, 88.0),   // Schlossplatz
    (48.8000, 9.1650, Scenario::Critical, 14.0), // Killesberg
    (48.7745, 9.1668, Scenario::Normal, 71.0),   // Feuersee
    (48.8008, 9.0145, Scenario::Normal, 64.0),   // Leonberg Marktplatz
    (48.8063, 9.0090, Scenario::GoesOffline { after_rounds: 6 }, 70.0), // Leonberg Bahnhof
    (48.8010, 9.0330, Scenario::Normal, 52.0),   // Leonberg Ost
    (48.8265, 9.0680, Scenario::Normal, 83.0),   // Ditzingen
    (48.8095, 9.2750, Scenario::NoGps, 62.0),    // Fellbach
    (48.7515, 9.1700, Scenario::Normal, 45.0),   // Degerloch
    (48.7260, 9.1130, Scenario::Normal, 91.0),   // Vaihingen
    (48.8050, 9.2180, Scenario::Normal, 33.0),   // Bad Cannstatt
];

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
        let mut rng = Rng(config.seed);
        let nodes = NODES
            .iter()
            .enumerate()
            .map(|(i, &(lat, lon, scenario, soc))| SimNode {
                id: format!("SIM{:02}", i + 1),
                lat,
                lon,
                scenario,
                soc,
                base_current_ma: 250.0 + rng.unit() * 170.0,
                phase: rng.unit() * std::f64::consts::TAU,
                rounds: 0,
            })
            .collect();
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

            let current_ma = node.base_current_ma * (1.0 + 0.1 * rng.signed());
            let scaled_ma = current_ma * cfg.speedup;
            node.soc =
                (node.soc - 100.0 * (scaled_ma / 1000.0) * cfg.interval_secs / 3600.0 / cfg.capacity_ah).max(0.0);

            let rest_mv = f64::from(cfg.min_mv) + node.soc / 100.0 * f64::from(cfg.max_mv - cfg.min_mv);
            let sag_mv = current_ma / 1000.0 * 50.0;
            let voltage_mv = (rest_mv - sag_mv + 20.0 * rng.signed()).max(1_000.0);
            let temperature_c = 10.0 + 8.0 * (f64::from(node.rounds) * 0.1 + node.phase).sin() + 0.5 * rng.signed();

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
pub fn estimated_soc(voltage_mv: i32, config: &FleetConfig) -> f32 {
    calculate_soc(voltage_mv, config.min_mv, config.max_mv)
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
    fn has_twelve_simulated_nodes() {
        let fleet = Fleet::new(only_valid());
        assert_eq!(fleet.node_count(), 12);
        assert_eq!(fleet.node_ids()[0], "SIM01");
        assert_eq!(fleet.node_ids()[11], "SIM12");
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
    fn one_node_starts_critical() {
        let cfg = only_valid();
        let mut fleet = Fleet::new(cfg.clone());
        let socs: Vec<f32> = fleet
            .tick()
            .iter()
            .map(|m| estimated_soc(parse(m).voltage_mv, &cfg))
            .collect();
        assert_eq!(socs.iter().filter(|s| **s < 20.0).count(), 1);
        assert!(socs[2] < 20.0);
    }

    #[test]
    fn one_node_never_sends_a_position() {
        let mut fleet = Fleet::new(only_valid());
        for _ in 0..5 {
            let round = fleet.tick();
            let without: Vec<_> = round.iter().filter(|m| !parse(m).has_position()).collect();
            assert_eq!(without.len(), 1);
            assert!(without[0].topic.contains("SIM09"));
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
