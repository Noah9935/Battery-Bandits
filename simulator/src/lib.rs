//! Simulation einer Flotte von TrafficNodes.
//!
//! Die Flotte ist rein und deterministisch: Mit demselben Startwert entstehen
//! dieselben Nachrichten. Dadurch lassen sich Last- und Fehlertests
//! wiederholen. Ein kleiner Teil der Nachrichten ist absichtlich fehlerhaft,
//! um die Validierung (F06) zu prüfen.

use serde_json::json;

/// Einfacher, deterministischer Zufallsgenerator (SplitMix64).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Gleichverteilt in [0, 1)
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    charge_pct: f64,
    drain_pct_per_day: f64,
}

/// Eine zu sendende MQTT-Nachricht.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
    /// Ob die Nachricht gültig sein soll. Fehlerhafte sind gewollt (F06).
    pub valid: bool,
}

pub const DAY_MS: f64 = 24.0 * 3600.0 * 1000.0;
/// Unter diesem Ladestand wird die Batterie "gewechselt" und springt auf 100 Prozent.
const REPLACE_BELOW_PCT: f64 = 3.0;

#[derive(Debug, Clone)]
pub struct Fleet {
    nodes: Vec<Node>,
    rng: Rng,
    invalid_rate: f64,
    time_scale: f64,
}

impl Fleet {
    /// * `invalid_rate`: Anteil absichtlich fehlerhafter Nachrichten, 0 bis 1
    /// * `time_scale`: Beschleunigung der Entladung, 1 ist Echtzeit
    pub fn new(count: usize, seed: u64, invalid_rate: f64, time_scale: f64) -> Self {
        let mut rng = Rng::new(seed);
        let nodes = (1..=count)
            .map(|n| Node {
                id: format!("TN-{n:04}"),
                // Gemischter Anfangszustand, damit Übersicht und Filter von Anfang an etwas zeigen.
                charge_pct: rng.range(8.0, 100.0),
                drain_pct_per_day: rng.range(0.5, 4.0),
            })
            .collect();
        Self {
            nodes,
            rng,
            invalid_rate: invalid_rate.clamp(0.0, 1.0),
            time_scale: time_scale.max(0.0),
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Lässt `elapsed_ms` vergehen und erzeugt für jedes Gerät eine Nachricht.
    pub fn step(&mut self, now_ms: i64, elapsed_ms: i64) -> Vec<Message> {
        let days = elapsed_ms.max(0) as f64 * self.time_scale / DAY_MS;
        let mut messages = Vec::with_capacity(self.nodes.len());
        for index in 0..self.nodes.len() {
            let noise = self.rng.range(-0.05, 0.05);
            let node = &mut self.nodes[index];
            node.charge_pct -= node.drain_pct_per_day * days;
            if node.charge_pct <= REPLACE_BELOW_PCT {
                node.charge_pct = 100.0;
            }
            node.charge_pct = (node.charge_pct + noise).clamp(0.0, 100.0);

            let id = node.id.clone();
            let charge = (node.charge_pct * 10.0).round() / 10.0;
            let topic = format!("trafficnode/{id}/battery");
            let message = if self.rng.unit() < self.invalid_rate {
                self.invalid(topic, &id, now_ms)
            } else {
                let payload = json!({ "device_id": id, "timestamp": now_ms, "charge_pct": charge });
                Message {
                    topic,
                    payload: payload.to_string().into_bytes(),
                    valid: true,
                }
            };
            messages.push(message);
        }
        messages
    }

    fn invalid(&mut self, topic: String, id: &str, now_ms: i64) -> Message {
        let payload: Vec<u8> = match (self.rng.unit() * 5.0) as u32 {
            0 => b"{kaputt".to_vec(),
            1 => json!({ "device_id": id, "timestamp": now_ms, "charge_pct": 150.0 })
                .to_string()
                .into_bytes(),
            2 => json!({ "device_id": id, "timestamp": now_ms }).to_string().into_bytes(),
            3 => json!({ "device_id": "TN-9999", "timestamp": now_ms, "charge_pct": 50.0 })
                .to_string()
                .into_bytes(),
            _ => json!({ "device_id": id, "timestamp": now_ms, "charge_pct": "voll" })
                .to_string()
                .into_bytes(),
        };
        Message {
            topic,
            payload,
            valid: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use shared::validation::{Rules, parse_message};

    use super::*;

    const NOW: i64 = 1_793_000_000_000;

    #[test]
    fn same_seed_gives_same_messages() {
        let mut a = Fleet::new(20, 7, 0.2, 1.0);
        let mut b = Fleet::new(20, 7, 0.2, 1.0);
        assert_eq!(a.step(NOW, 300_000), b.step(NOW, 300_000));
    }

    #[test]
    fn different_seeds_differ() {
        let mut a = Fleet::new(20, 1, 0.0, 1.0);
        let mut b = Fleet::new(20, 2, 0.0, 1.0);
        assert_ne!(a.step(NOW, 0), b.step(NOW, 0));
    }

    #[test]
    fn one_message_per_device_with_expected_topic() {
        let mut fleet = Fleet::new(500, 1, 0.0, 1.0);
        let messages = fleet.step(NOW, 300_000);
        assert_eq!(messages.len(), 500);
        assert_eq!(messages[0].topic, "trafficnode/TN-0001/battery");
        assert_eq!(messages[499].topic, "trafficnode/TN-0500/battery");
    }

    #[test]
    fn valid_messages_pass_the_real_validation() {
        let mut fleet = Fleet::new(200, 3, 0.0, 1.0);
        for m in fleet.step(NOW, 300_000) {
            assert!(m.valid);
            let r = parse_message(&m.topic, &m.payload, NOW, &Rules::default()).expect("gültig");
            assert!((0.0..=100.0).contains(&r.charge_pct));
        }
    }

    #[test]
    fn invalid_messages_are_really_rejected() {
        let mut fleet = Fleet::new(300, 4, 1.0, 1.0);
        let messages = fleet.step(NOW, 300_000);
        assert!(messages.iter().all(|m| !m.valid));
        for m in messages {
            assert!(
                parse_message(&m.topic, &m.payload, NOW, &Rules::default()).is_err(),
                "{:?}",
                String::from_utf8_lossy(&m.payload)
            );
        }
    }

    #[test]
    fn invalid_rate_is_roughly_respected() {
        let mut fleet = Fleet::new(2000, 5, 0.1, 1.0);
        let invalid = fleet.step(NOW, 300_000).iter().filter(|m| !m.valid).count();
        assert!((120..=280).contains(&invalid), "{invalid} fehlerhafte von 2000");
    }

    #[test]
    fn batteries_drain_and_get_replaced() {
        let mut fleet = Fleet::new(1, 6, 0.0, 1.0);
        let start = fleet.nodes[0].charge_pct;
        fleet.step(NOW, 24 * 3600 * 1000);
        assert!(fleet.nodes[0].charge_pct < start, "Ladestand muss sinken");

        // Zehn Jahre am Stück: die Batterie ist leer und wird ersetzt.
        fleet.step(NOW, 3650 * 24 * 3600 * 1000);
        assert!(fleet.nodes[0].charge_pct > 90.0, "Batterie muss ersetzt worden sein");
    }

    #[test]
    fn time_scale_speeds_up_draining() {
        let mut slow = Fleet::new(1, 8, 0.0, 1.0);
        let mut fast = Fleet::new(1, 8, 0.0, 10.0);
        let start = slow.nodes[0].charge_pct;
        slow.step(NOW, 3_600_000);
        fast.step(NOW, 3_600_000);
        assert!(start - fast.nodes[0].charge_pct > start - slow.nodes[0].charge_pct);
    }

    #[test]
    fn charge_stays_in_range() {
        let mut fleet = Fleet::new(100, 9, 0.0, 50.0);
        for i in 0..200 {
            for m in fleet.step(NOW + i * 1000, 60_000) {
                let r = parse_message(&m.topic, &m.payload, NOW + i * 1000, &Rules::default()).unwrap();
                assert!((0.0..=100.0).contains(&r.charge_pct));
            }
        }
    }

    #[test]
    fn rng_is_uniform_enough() {
        let mut rng = Rng::new(1);
        let mean = (0..10_000).map(|_| rng.unit()).sum::<f64>() / 10_000.0;
        assert!((0.48..0.52).contains(&mean));
    }
}
