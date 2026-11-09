# CLAUDE.md: Battery Bandits (TrafficNode Monitoring)

## 1. Projektziel & Architektur
Battery Bandits ist eine Web-Applikation zur Batterie- und Telemetrieüberwachung mobiler TrafficNodes.
Das System ist als **Rust Cargo Workspace in einer Event-getriebenen Microservice-Architektur** aufgebaut (basierend auf unserem Event Storming):

### Workspace-Struktur
- `crates/shared`: Gemeinsame Datentypen, DTOs, Domain-Events (`MessageReceived`, `ReadingSaved`, `DeviceCritical`, `DeviceOffline`, `DeviceRecovered`, `AlertCreated`, `BatteryReplaced`), Fehlerbehandlung und Konfigurations-Structs.
- `services/ingestion-service` (Erfassungs-Service): Abonniert den MQTT-Broker (`rumqttc`), validiert eingehende Roh-Nachrichten, verwirft/loggt ungültige Payloads und speichert gültige Messwerte in TimescaleDB. Feuert `ReadingSaved`-Events (via Postgres NOTIFY / MQTT).
- `services/monitoring-service` (Überwachungs-Service): Reagiert auf `ReadingSaved`, prüft Schwellwerte (Kritisch-Markierung) und prüft per Hintergrund-Timer (`tokio::time`) zyklisch auf inaktive Geräte (Offline-Erkennung). Aktualisiert den Gerätestatus (`Normal`, `Critical`, `Offline`).
- `services/operations-service` (Meldungs- & Einsatz-Service): Verwaltet Meldungen bei kritischen/offline Geräten (Eröffnen, Abweisen) sowie die Einsatzplanung (Batteriewechsel planen, Wechsel bestätigen, setzt Kritisch-Status zurück).
- `services/dashboard-service` (Dashboard-Service / API-Gateway): Axum-Server mit Read-Modellen (CQRS), REST-API für das Frontend (Geräteübersicht, Filter/Sortierung, Zeitreihen-Verlauf, Karte, CSV-Export), Weiterleitung von Kommandos an den `operations-service` und Echtzeit-Push an den Browser via Server-Sent Events (SSE, Latenz unter 30s).
- `services/test-publisher`: Eigenständiger MQTT-Simulator zum Senden realistischer TrafficNode-Telemetriedaten inkl. Entladekurven und Fehlerszenarien.
- `frontend`: Rust Leptos (CSR/WASM, gebaut mit Trunk) Single-Page-Application.

## 2. Datenformat der TrafficNodes (MQTT Payload)
Topic-Muster: `trafficnode/{node_id}/telemetry`
JSON-Payload:
```json
{
  "current_mA": 0,
  "voltage_mV": 0,
  "lat": null,
  "lon": null,
  "altitude_m": null,
  "satellites": null,
  "time": null,
  "temperature_C": 0
}
```

---

# Ergänzungen aus Vorlesung und Datenprüfung (06.11.2026)

Der Text oberhalb ist die Vorgabe von Noah. Die folgenden Punkte ergänzen ihn mit geprüften Fakten. Bei Widersprüchen gilt: **Vorlesung und Dozent, dann der Text oberhalb, dann diese Ergänzungen, dann README.** Wo der Text oberhalb und die echten Daten abweichen, bleibt der Text oberhalb der Standard und die Abweichung ist per Konfiguration umschaltbar.

## A. Echte Datenquelle
* Das Live-Topic auf `mqtt.ti-labs.ai:8883` (TLS ab 1.2, Zertifikat und Hostname prüfen) heißt `traffic-nodes/<Seriennummer>/measurements`, zum Beispiel `TN0001`. Die Nutzlast entspricht dem Format oben (`sample_msg.json`), Spannung um 12,3 V.
* **Entscheidung (06.11.2026, Noah): Vorlesung und Server gehen vor.** Der Standard ist überall das echte Topic `traffic-nodes/+/measurements`, auch im Test-Publisher und in der Dev-Umgebung. Das Topic aus dem Text oben (`trafficnode/{node_id}/telemetry`) bleibt nur per `MQTT_TOPIC_FILTER` wählbar. Die `node_id` ist immer das zweite Topic-Segment, Geräte heißen wie in den echten Daten `TN0001` und so weiter.
* Zugangsdaten stehen in der vertraulichen YAML der Gruppe und kommen nur über Umgebungsvariablen in die Dienste, nie ins Repository, nie in den Chat oder in Logs.
* Historie lesend per SQL: Datenbank `measurements`, Schema `history`, Tabellen `mqtt_messages` und `measurements` (auf dem Server `127.0.0.1:5432`, nur Lesezugriff). Nützlich für Tests mit echten Werten.
* Batteriewechsel und Aussortierung erfasst die App BatteryTrack des Dozenten (`battery.ti-labs.ai`) und versendet sie per MQTT. Das Topic ist noch nicht bekannt, die Formate stehen in `exchange.example.json` und `discard.example.json`. Bis dahin löst nur die Oberfläche `BatteryReplaced` aus.

## B. Ladezustand nach Vorlesung "Einschub Batteriemessung"
* 12-V-Batterie, **80 Ah Nennkapazität**. Entnommene Ladung per Trapezregel `ΔQ = (I1 + I2) / 2 · Δt`, aufsummiert zu `Q(t)`. `SOC(t) = SOC(Start) − 100 · Q(t) / 80 Ah`.
* Startwert nach Batteriewechsel: Schätzung aus der Ruhespannung, linear zwischen `min_voltage_mv` und `max_voltage_mv` (Vorgabe 10500 und 12700 mV, einstellbar in `system_settings`). Das ist eine Annahme und im Code so gekennzeichnet.
* `calculate_soc(voltage_mv, min_mv, max_mv) -> f32` (reine Spannungsschätzung, wie im Phasenplan) und eine zweite reine Funktion für die Ladungsintegration.
* Das Vorzeichen des Stroms ist uneinheitlich (live negativ, Vorlesungsdaten positiv). Gerechnet wird mit dem Betrag der Entladung, die Richtung ist per Einstellung umkehrbar.
* Schwellwerte einstellbar: Kritisch unter 20 %, Warnung unter 40 %, Offline nach `offline_timeout_seconds`.

## C. Phasenplan und Ereignisse
* Gebaut wird in sieben Phasen: 1 Workspace, `shared`, Migrationen, Dev-Umgebung. 2 Ingestion-Service plus Test-Publisher. 3 Monitoring. 4 Operations. 5 Dashboard-Service. 6 Leptos-Frontend. 7 Deployment (cargo-chef, Compose, Caddy). Eine Phase ist erst fertig, wenn Build, Clippy und Tests grün sind.
* Ereignisse laufen über Postgres `NOTIFY`/`LISTEN` auf dem Kanal `domain_events` (JSON). Der Phasenplan kennt `ReadingSaved`, `DeviceStatusChanged`, `AlertCreated`, `BatteryReplaced`. Die feineren Event-Storming-Namen aus Abschnitt 1 (`DeviceCritical`, `DeviceOffline`, `DeviceRecovered`) sind Ausprägungen von `DeviceStatusChanged` und werden als Hilfs-Konstruktoren und Anzeigenamen geführt, nicht als eigene Kanäle.
* Statusmodell im Plan: `Normal`, `Warning`, `Critical`, `Offline` (mit Warnung, die im Text oben noch fehlt).

## D. Qualität (Vorlesung Testing und DevOps)
* Testpyramide 70/20/10, Ziel 80 % Abdeckung der Kernlogik (`cargo llvm-cov`), AAA, deterministisch und hermetisch, Kernlogik als reine Funktionen, Fakes statt Netzwerk in Unit-Tests, ephemere Container für Integrationstests.
* CI mit GitHub Actions: Format, Clippy, Tests, Abdeckung, Image-Bau.

## E. Entwicklung und Betrieb
* **Alles in Docker**, auf dem Rechner ist nichts außer Docker installiert. LF-Zeilenenden (`.gitattributes`). Commits nur lokal, gepusht wird nur auf ausdrückliche Anweisung. Keine Geheimnisse im Repository.
* Schulserver (Alias `bb`, Gruppe `tinf25d-battery-bandits`), Portbereich 20200 bis 20299, alles an `127.0.0.1`: prod Web 20200 und Datenbank 20201, staging Web 20210 und Datenbank 20211. Der zentrale Caddy des Dozenten (80 und 443) kennt `batterybandits.racore.net` und `staging.batterybandits.racore.net` noch nicht. Das richtet der Dozent ein und bleibt offen.
* Im Produktivbetrieb liefert der gemeinsame Broker `mqtt.ti-labs.ai` die Daten. Ein eigener Mosquitto gehört in die Dev- und Testumgebung.

## F. Oberfläche und Textregeln
* Minimalistisch, responsiv, nur Deutsch. **Design: Leitstand-Stil (Linear, Grafana, Raycast), Regeln in `.claude/commands/ui-polish.md`.** Schriften: Systemschrift für Text, Festbreitenschrift (`.mono-num`) für ALLE Zahlen. Flächen mit 1px-Rahmen, Radius 4px, keine Schatten und keine Verläufe, einfarbige Lucide-Icons (Strichstärke 1,75). Farb-Token für hell, dunkel, hohen Kontrast und Kompakt stehen am Anfang von `frontend/style.css`. Status: normal grün, Warnung gelb, kritisch rot, offline grau mit gestricheltem Rand. Seiten: Übersicht, Standortkarte, Wartung (Reiter Einsätze, Tour, Protokoll), Postfach, Optionen, dazu die eigene Geräteseite `/geraete/:id`. Anmeldung ist Pflicht (Konten aus `BB_USERS`). Zusatzfunktionen sind einzeln im Labor (Optionen) schaltbar (`frontend/src/features.rs`).
* **Keine Gedankenstriche** und **keine Emojis** in Dokumenten, Oberflächentexten, Kommentaren und Commit-Nachrichten. Code-Bezeichner englisch, Texte für Menschen deutsch.

## G. Offene Punkte
* Topic der Wechsel- und Aussortierereignisse von BatteryTrack.
* Hostnamen im zentralen Caddy (Dozent).
* Stromrichtung, Ruhespannungsgrenzen und Schwellwerte mit dem Auftraggeber bestätigen. Aufbewahrungsdauer der Messwerte (Vorgabe 12 Monate).

## H. Datenherkunft (Stand 06.11.2026)
Jedes Gerät trägt `devices.data_source`: `dozent` (echt: Messdatenplattform, Vorlesung) oder `simulation` (erfunden, nur der lokale Test-Publisher, Kennungen `SIM01` bis `SIM12`). Die Oberfläche markiert beides ("Dozent", "Erfunden"), filtert danach und zeigt es auf der Karte (gefüllter Punkt gegen gestrichelter Ring). Der Ingestion-Service setzt die Herkunft über `DATA_SOURCE` (Vorgabe `dozent`, lokal `simulation`). Echte Daten in Staging: Geräte `101` und `TN0001` aus `history.measurements` und `TSN-002` aus der Vorlesungs-CSV, eingespielt mit `import-history`.
