# Battery Bandits: Architekturkonzept

> **Stand:** 6. November 2026
> **Grundlage:** [README](../README.md) (Spezifikation und Entscheidungen), [CLAUDE.md](../CLAUDE.md) (Arbeitsgrundlage mit Vorlesungsvorgaben), [Event Storming](images/EventStorming_BatteryBandits.pdf), [UI-Mockup](TrafficNode_UI_Mockup.html)
> **Umsetzung:** alle sieben Phasen gebaut, lauffähig mit `deploy/start-local.sh` (siehe [Deployment](deployment.md))

Dieses Dokument beschreibt die Architektur so, wie sie umgesetzt ist, und begründet, warum sie so geschnitten wurde. Die kurzen Entscheidungsprotokolle stehen in der README (Abschnitt 7), die Diagramme in [docs/diagrams](diagrams/architecture/).

---

## 1. Entscheidungen auf einen Blick

| Thema | Entscheidung | Warum in einem Satz |
|-------|--------------|---------------------|
| **Architekturstil** | Microservices nach dem Event Storming, je Dienst ein Container | Die fachlichen Schnitte (Erfassung, Überwachung, Meldungen und Einsatz, Dashboard) sind geklärt, und das Team will sie als eigenständige Dienste üben |
| **Sprache** | Rust in allen Diensten und im Browser (Leptos, WebAssembly) | Eine Sprache von der Datenbank bis zur Oberfläche, gemeinsame Typen im Paket `shared` |
| **Ereignisse** | PostgreSQL `NOTIFY` und `LISTEN` auf dem Kanal `domain_events` (JSON) | Keine zusätzliche Komponente im Betrieb, Ereignis und Datenänderung liegen in derselben Transaktion |
| **Datenbank** | PostgreSQL mit TimescaleDB, eine Instanz | Zeitreihen und relationale Daten zusammen, wie in der Vorlesung für die eigene Anwendung vorgesehen (C4 Ebene 1) |
| **Datenquelle** | Gemeinsamer MQTT-Broker der Messdatenplattform, Topic `traffic-nodes/<Seriennummer>/measurements` | Vorgabe der Vorlesung und des Servers, lokal ersetzt ein Mosquitto den Broker |
| **Ladezustand** | Stromintegration nach der Vorlesung "Einschub Batteriemessung", Startwert aus der Ruhespannung | Gleicher Verbrauch ergibt je nach Startwert einen anderen Restladezustand |
| **Live-Updates** | Server-Sent Events | Neue Werte erscheinen nach wenigen Sekunden (NF01), einfacher als WebSockets |
| **Web-Auslieferung** | Caddy: statische Dateien plus Weiterleitung von `/api`, SSE ohne Zwischenspeicher | Ein kleiner Container ohne Nginx-Konfiguration |
| **Betrieb** | Docker Compose, GitHub Actions, Images mit cargo-chef | Von der Aufgabenstellung vorgegeben (NF06) |

---

## 2. Ausgangslage und Architekturtreiber

Die Last ist klein, die Datenmenge über ein Jahr mittelgroß, das Team klein und die Zeit knapp. Microservices sind deshalb keine Notwendigkeit der Last, sondern eine bewusste Entscheidung des Teams.

| Treiber | Wert | Folge für die Architektur |
|---------|------|---------------------------|
| **Geräte** | ca. 500 | Die Geräteliste passt komplett in den Speicher des Browsers, Filtern und Sortieren (F13) laufen dort |
| **Nachrichten** | ca. 144.000 pro Tag, also rund 1,7 pro Sekunde | Ein einzelner Empfangsprozess reicht um Größenordnungen |
| **Aufbewahrung** | 12 Monate | rund 52 Mio. Messwerte: Hypertable, Kompression nach 7 Tagen und Löschen nach 12 Monaten sind Richtlinien der Datenbank |
| **Gleichzeitige Nutzende** | höchstens 20 | Ein Dashboard-Prozess genügt, Server-Push ist unproblematisch |
| **NF01 Performance** | neuer Wert innerhalb von 30 s, Ziel 10 s im Dashboard | Push vom Server in den Browser, Drosselung des Nachladens auf 2 Sekunden |
| **NF02 Skalierbarkeit** | Last ohne Nachrichtenverlust | QoS 1, feste Sitzung, Bestätigung erst nach der Verarbeitung |
| **NF03 Zuverlässigkeit** | Verbindung nach Broker-Neustart spätestens nach 30 s wieder da | Selbstständiges Wiederverbinden nach 2 Sekunden Wartezeit |
| **NF05 Testbarkeit** | Ziel 80 % Abdeckung der Kernlogik (Vorlesung), CI grün | Kernlogik als reine Funktionen, Testpyramide 70/20/10 |
| **NF06 Portabilität** | `docker compose up` startet alles | Alle Teile als Container, auf dem Rechner ist nur Docker nötig |
| **NF07 Wartbarkeit** | Schwellwerte ohne Codeänderung | `system_settings` in der Datenbank, über die Oberfläche änderbar |
| **Systemgrenze** | Broker und Messdatenplattform gehören dem Dozenten | Wir sind nur MQTT-Client und lesen die Historie höchstens per SQL |

---

## 3. Vom Event Storming zu den Diensten

| Service im Event Storming | Dienst im Code | Aufgabe | Anforderungen |
|---------------------------|----------------|---------|---------------|
| Erfassungs-Service | `services/ingestion-service` | MQTT empfangen, prüfen, speichern oder verwerfen | F01, F02, F06 |
| Überwachungs-Service | `services/monitoring-service` | Status bewerten, Offline erkennen, Restlaufzeit prognostizieren | F05, F07, F08 |
| Meldungs- und Einsatz-Service | `services/operations-service` | Meldungen eröffnen, abweisen, lösen. Batteriewechsel planen und bestätigen | Meldungen, Einsatzplanung |
| Dashboard-Service | `services/dashboard-service` und `frontend` | Lese-Modelle, Filter, Verlauf, Karte, CSV, Einstellungen, Echtzeit | F03, F04, F11, F12, F13, NF07 |
| Testquelle | `services/test-publisher` | 12 simulierte Knoten für Entwicklung und Tests | Aufgabenstellung A05 |

Dazu kommt das Paket `crates/shared` ohne eigenen Prozess: Nachrichtenformat und Prüfung, Ladezustand, Prognose, Modelle, Einstellungen, Ereignisse und API-Typen. Es wird vom Backend und vom WebAssembly-Frontend verwendet; Datenbankteile hängen am Feature `sqlx`.

Die Ereignisnamen aus dem Event Storming `DeviceCritical`, `DeviceOffline` und `DeviceRecovered` sind Ausprägungen des Ereignisses `DeviceStatusChanged` (`DomainEvent::storming_name`). So bleibt der Kanal klein und die Fachsprache erhalten.

---

## 4. Zusammenspiel der Dienste

### Ablauf einer Nachricht

1. Der Ingestion-Service empfängt die MQTT-Nachricht (QoS 1) und prüft sie mit `shared` (Format, Wertebereiche, Topic).
2. Der `MqttReceiver` ist das Subjekt im Observer-Muster und meldet das Ergebnis über den Trait `ReadingObserver` an seine Beobachter. Ungültig: Die Nachricht landet mit Grund in `invalid_messages_log`, kein Ereignis. Gültig: Der `PostgresObserver` schreibt in einer Transaktion Messwert und Gerätestand (Ladezustand fortgeschrieben, bekannte Position bleibt bei fehlendem GPS-Fix erhalten) und sendet `ReadingSaved` per `NOTIFY`.
3. Erst nach der Verarbeitung bestätigt der Dienst die Nachricht beim Broker. Fällt er vorher aus, stellt der Broker sie erneut zu.
4. Der Monitoring-Service reagiert auf `ReadingSaved`, setzt den Status (`Normal`, `Warning`, `Critical`, `Offline`) und sendet bei einer Änderung `DeviceStatusChanged`. Alle 15 Sekunden bewertet er zusätzlich alle Geräte neu: Das erkennt Offline, übernimmt geänderte Schwellwerte und holt Verpasstes nach. Jede Minute schreibt er die Restlaufzeit-Prognose fort.
5. Der Operations-Service eröffnet bei `Critical` oder `Offline` genau eine Meldung je Gerät (`AlertCreated`) und löst sie bei `Normal` oder `Warning`. Über REST lassen sich Meldungen abweisen und Batteriewechsel planen und bestätigen. Die Bestätigung sendet `BatteryReplaced`.
6. Bei `BatteryReplaced` beginnt ein neuer Entladezyklus: Der Monitoring-Service setzt die Integration zurück, der Startwert wird mit der nächsten Messung aus der Spannung geschätzt.
7. Der Dashboard-Service hört auf alle Ereignisse und schickt sie per Server-Sent Events an die Browser. Diese laden die betroffenen Daten gedrosselt neu.

### Ereignisse

| Name | Sender | Empfänger |
|------|--------|-----------|
| `ReadingSaved` | Ingestion | Monitoring, Dashboard |
| `DeviceStatusChanged` | Monitoring | Operations, Dashboard |
| `AlertCreated` | Operations | Dashboard |
| `BatteryReplaced` | Operations (später auch BatteryTrack) | Monitoring, Operations, Dashboard |

Kommandos des Browsers (Meldung abweisen, Wechsel planen und bestätigen) gehen über den Dashboard-Service als HTTP-Aufruf an den Operations-Service. Nur der Besitzer eines Datenbestands schreibt ihn; der Dashboard-Service schreibt allein `system_settings`.

### Zuverlässigkeit

| Situation | Verhalten |
|-----------|-----------|
| Dashboard startet neu | Der Empfang läuft ungestört weiter (NF03), Browser verbinden sich selbst neu und laden nach |
| Monitoring oder Operations ist kurz aus | `NOTIFY` wird nicht gespeichert. Monitoring bewertet beim Start und danach alle 15 Sekunden alle Geräte neu. Operations gleicht beim Start und nach einer gestörten Verbindung ab (fehlende Meldungen eröffnen, überholte lösen) |
| Broker-Neustart | Der Ingestion-Service verbindet sich selbst neu (Wartezeit 2 s), die Sitzung bleibt bestehen |
| Datenbank kurz nicht erreichbar | Die Verarbeitung wird bis zu dreimal wiederholt, die Nachricht wird nicht bestätigt, solange sie nicht gespeichert ist |
| Verspätete Messung | Wird gespeichert, ändert aber den aktuellen Gerätestand nicht |
| Browser verliert Ereignisse | Der Server sendet das Ereignis `resync`, der Browser lädt alles neu |

Hinweis: `NOTIFY` ist anders als ein Nachrichtenbus nicht dauerhaft. Das ist hier tragbar, weil jeder Empfänger den Zustand aus der Datenbank abgleichen kann; die Ereignisse sind Auslöser, nicht die einzige Wahrheit.

---

## 5. Datenmodell

Alle Dienste teilen eine Datenbank. Die Migrationen liegen zentral in `migrations/` und werden vom Paket `shared` eingebettet; jeder Dienst wendet sie beim Start an (sqlx sperrt per Advisory Lock).

| Tabelle | Besitzer (schreibt) | Wichtige Felder | Zweck |
|---------|---------------------|-----------------|-------|
| `devices` | Ingestion (Messwerte), Monitoring (Status, Prognose) | `id`, `status`, `soc_percent`, Messwerte, Position, `last_seen_at`, `soc_start_percent`, `discharged_ah`, `battery_installed_at`, `discharge_rate_pct_per_hour` | Letzter bekannter Zustand je TrafficNode |
| `readings` | Ingestion | `time`, `device_id`, Spannung, Strom, Temperatur, `soc_percent`, Position | Alle gültigen Messwerte (F02), Hypertable, Kompression nach 7 Tagen, Aufbewahrung 12 Monate |
| `invalid_messages_log` | Ingestion | `received_at`, `topic`, `payload`, `reason` | Verworfene Nachrichten (F06) |
| `alerts` | Operations | `device_id`, `kind`, `status`, `created_at`, `closed_at` | Meldungen: offen, abgewiesen, gelöst |
| `maintenance_tasks` | Operations | `device_id`, `alert_id`, `status`, `note`, `completed_at` | Geplante und erledigte Batteriewechsel |
| `system_settings` | Dashboard (über die Oberfläche) | Schwellwerte, Offline-Zeit, Spannungsgrenzen, Nennkapazität | Genau eine Zeile (NF07) |

Regeln, die die Datenbank absichert: höchstens eine offene Meldung je Gerät und höchstens ein geplanter Einsatz je Gerät (partielle Unique-Indizes), Position nur als Paar, Schwellwerte in sinnvoller Reihenfolge.

### Ladezustand

Nach der Vorlesung "Einschub Batteriemessung":

* Entnommene Ladung per Trapezregel: `ΔQ = (I1 + I2) / 2 · Δt`, aufsummiert zu `Q(t)`.
* `SOC(t) = SOC(Start) − 100 · Q(t) / 80 Ah` (12-V-Batterie, Nennkapazität einstellbar).
* Das Vorzeichen des Stroms ist uneinheitlich (live negativ, Vorlesungsdaten positiv), deshalb zählt der Betrag.
* **Annahme, nicht aus der Vorlesung:** Den Startwert nach einem Batteriewechsel schätzen wir linear aus der Ruhespannung zwischen 10,5 und 12,7 V (einstellbar).

### Prognose

Lineare Ausgleichsgerade über den Ladezustand der letzten 24 Stunden, höchstens seit dem letzten Batteriewechsel, mindestens drei Messwerte über zehn Minuten. Ergebnis: Entladerate in Prozentpunkten pro Stunde und Stunden bis leer. Das Dashboard rechnet daraus die Zeit bis zum kritischen Wert.

---

## 6. Technologieentscheidungen

| Baustein | Wahl | Hinweis |
|----------|------|---------|
| Async-Laufzeit | tokio | Standard, von allen anderen Bausteinen vorausgesetzt |
| MQTT | rumqttc | QoS 1, feste Sitzung, manuelle Bestätigung, TLS mit rustls |
| HTTP und SSE | Axum | Passt zu tokio und tower, SSE ist eingebaut |
| Datenbank | SQLx | Abfragen zur Laufzeit, getestet gegen eine echte TimescaleDB in ephemeren Datenbanken (`sqlx::test`) |
| Frontend | Leptos (Client-seitig), Trunk | Gemeinsame Typen mit dem Backend. Diagramm als eigenes SVG, Karte mit Leaflet über JavaScript-Interop (lokal eingebunden) |
| Images | cargo-chef, mehrstufig | Abhängigkeiten in einer eigenen Schicht, schlanke Laufzeit-Images ohne Root |

Zur Frage "Warum nicht NATS?": In einer früheren Fassung trug NATS JetStream die Ereignisse. Mit dem Phasenplan der Vorlesung wurde auf `NOTIFY` und `LISTEN` umgestellt. Das spart einen Baustein im Betrieb. Der Preis ist die fehlende Speicherung der Ereignisse, den der Abgleich der Dienste (siehe Zuverlässigkeit) ausgleicht. Ein Wechsel zu einem echten Bus bliebe möglich, weil nur `shared::db::notify_event` und die Hörer-Schleifen betroffen wären.

---

## 7. Qualitätssicherung

Nach der Vorlesung (Testing und DevOps): Testpyramide mit der Faustformel 70/20/10, Arrange Act Assert, deterministische und hermetische Unit-Tests, Fakes statt Netzwerk, ephemere Container für Integrationstests, Ziel 80 % Abdeckung der Kernlogik.

| Ebene | Beispiele |
|-------|-----------|
| Unit-Tests (rein) | Nachrichtenprüfung, Ladezustand, Prognose, Statusregeln, Meldungsregeln, Filter und Sortierung, Diagramm-Geometrie, Formatierung, CSV |
| Komponenten und Integration | Datenbank-Beobachter, Statusbewertung, Meldungen, HTTP-Schnittstellen (jeweils gegen eine echte TimescaleDB), MQTT-Durchstich über Mosquitto, Proxy mit nachgebautem Dienst, SSE über TCP |
| Durchstich (wenige) | `tools/smoke.sh` gegen den vollständigen Stack mit Simulator |

CI in GitHub Actions: Format, Clippy (nativ und WebAssembly), Tests mit TimescaleDB und Mosquitto, Abdeckung, danach Bau und Start des gesamten Stacks.

---

## 8. Nicht umgesetzt (Future Work)

| Anforderung | Plan |
|-------------|------|
| Ereignisse von BatteryTrack | Der Dozent erfasst Wechsel und Aussortierung und versendet sie per MQTT (Formate `exchange` und `discard` liegen vor). Sobald das Topic bekannt ist, wird ein weiterer Beobachter im Ingestion-Service daraus `BatteryReplaced` erzeugen |
| F09 Benachrichtigung | Neuer Abonnent von `AlertCreated`, zum Beispiel ein Dienst mit SMTP |
| F10 Login | Eigene Konten mit Session-Cookie im Dashboard-Service. NF09 sieht gleichberechtigte Nutzende vor |
| Tourenplanung | Weiterer Dienst, abonniert die Einsatzereignisse |
| Abgleich des Ladezustands | Die Integration driftet über lange Zeit. Möglich wäre eine Neubestimmung aus der Ruhespannung bei sehr kleinem Strom |

---

## 9. Annahmen und offene Fragen

| Frage | Annahme im Code | Wo anpassen |
|-------|-----------------|-------------|
| Topic | `traffic-nodes/+/measurements` (echtes Topic), das Topic aus dem Phasenplan `trafficnode/+/telemetry` per Konfiguration | `MQTT_TOPIC_FILTER` |
| Stromvorzeichen | Betrag zählt | `shared/src/soc.rs` |
| Startwert des Ladezustands | Linear aus der Ruhespannung, 10,5 bis 12,7 V | `system_settings`, `shared/src/soc.rs` |
| Schwellwerte | Kritisch unter 20 %, Warnung unter 40 %, Offline nach 15 Minuten | Oberfläche (Wartung und Optionen) |
| Aufbewahrung | 12 Monate | `migrations/0001_init.sql` |
| Topic der Batteriewechsel von BatteryTrack | unbekannt | Dozent |
| Domain im zentralen Caddy | Stand 06.11.2026 nicht eingerichtet | Dozent, siehe [Deployment](deployment.md) |
