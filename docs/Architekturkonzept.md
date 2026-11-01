# Battery Bandits: Architekturkonzept

> **Stand:** 1. November 2026
> **Grundlage:** [README](../README.md) (Spezifikation und Entscheidungen), [Event Storming](images/EventStorming_BatteryBandits.pdf), [UI-Mockup](TrafficNode_UI_Mockup.html)
> **Umsetzung:** MVP ist gebaut und lauffähig (`docker compose --profile dev up --build`)

Dieses Dokument beschreibt die Architektur so, wie sie umgesetzt ist, und begründet, warum sie so geschnitten wurde. Die kurzen Entscheidungsprotokolle stehen in der README (Abschnitt 7), die Diagramme in [docs/diagrams](diagrams/architecture/).

---

## 1. Entscheidungen auf einen Blick

| Thema | Entscheidung | Warum in einem Satz |
|-------|--------------|---------------------|
| **Architekturstil** | Microservices nach dem Event Storming, je Dienst ein Container | Die fachlichen Schnitte (Erfassung, Überwachung, Meldungen, Dashboard) sind bereits geklärt, und das Team will sie als eigenständige Dienste üben |
| **Sprache** | Rust in allen Diensten und im Browser (Leptos, WebAssembly) | Eine Sprache von der Datenbank bis zur Oberfläche, gemeinsame Typen im Paket `shared` |
| **Ereignisbus** | NATS mit JetStream | Ein kleiner Container, Ereignisse werden gespeichert und gehen bei kurzen Ausfällen nicht verloren |
| **Datenbank** | PostgreSQL mit TimescaleDB, ein Schema je Dienst | Zeitreihen und relationale Daten in einer Instanz, trotzdem klare Besitzverhältnisse |
| **MQTT** | Broker des Auftraggebers; lokal Eclipse Mosquitto | Wir betreiben den Broker nicht (README), brauchen aber einen für Entwicklung und Simulator |
| **Live-Updates** | Server-Sent Events | Neue Werte erscheinen nach wenigen Sekunden (NF01), einfacher als WebSockets |
| **Web-Auslieferung** | Caddy: statische Dateien plus Weiterleitung von `/api` | Ein kleiner Container, ohne Nginx-Konfiguration |
| **Betrieb** | Docker Compose, GitHub Actions | Von der README vorgegeben (NF06) |

---

## 2. Ausgangslage und Architekturtreiber

Die Last ist klein, die Datenmenge über ein Jahr mittelgroß, das Team klein und die Zeit knapp. Microservices sind deshalb keine Notwendigkeit der Last, sondern eine bewusste Entscheidung des Teams. Das Konzept zeigt, wie man sie so baut, dass der Mehraufwand überschaubar bleibt.

| Treiber | Wert | Folge für die Architektur |
|---------|------|---------------------------|
| **Geräte** | ca. 500 | Die Geräteliste passt komplett in den Speicher des Browsers, Filtern und Sortieren (F13) laufen dort |
| **Nachrichten** | ca. 144.000 pro Tag, also rund 1,7 pro Sekunde | Ein einzelner Empfangsprozess reicht um Größenordnungen, kein Lastverteiler nötig |
| **Aufbewahrung** | 12 Monate | rund 52 Mio. Messwerte: Partitionierung, Kompression und automatisches Löschen lohnen sich |
| **Gleichzeitige Nutzende** | höchstens 20 | Ein API-Prozess genügt, Server-Push ist unproblematisch |
| **NF01 Performance** | neuer Wert höchstens 10 s im Dashboard | Push vom Server in den Browser statt Abfrage im Minutentakt |
| **NF02 Skalierbarkeit** | Last ohne Nachrichtenverlust | QoS 1, feste Sitzung, Bestätigung erst nach der Verarbeitung |
| **NF03 Zuverlässigkeit** | Broker-Neustart: Verbindung nach höchstens 30 s wieder da | Selbstständiges Wiederverbinden mit Wartezeit bis 10 s, Empfang getrennt von der Web-API |
| **NF05 Testbarkeit** | mindestens 70 % Abdeckung der Kernlogik, CI grün | Kernlogik als reine Funktionen in `shared`, ohne Datenbank und Netzwerk testbar |
| **NF06 Portabilität** | `docker compose up` startet alles | Alle Teile als Container, keine Cloud-Dienste |
| **NF07 Wartbarkeit** | Schwellwerte ohne Codeänderung | Schwellwert in der Datenbank, über die Oberfläche änderbar; Vorgabe per Umgebungsvariable |
| **Team und Zeit** | 3 Personen, 1 Semester | Eine Sprache, wenige und verbreitete Bausteine |
| **Systemgrenze** | Broker wird nicht von uns betrieben | Wir sind nur MQTT-Client |

Die Zahlen stammen aus der Aufgabenstellung. Die README (v0.2.1) nennt sie teilweise noch nicht oder anders (zum Beispiel NF01 mit 30 Sekunden) und sollte angeglichen werden.

---

## 3. Vom Event Storming zu den Diensten

Das Event Storming schneidet das System in fünf fachliche Services und hält fest, dass sie sich technisch als Module umsetzen lassen. Die Umsetzung macht aus jedem Service einen Dienst:

| Service im Event Storming | Dienst im Code | Aufgabe | Anforderungen | Stand |
|---------------------------|----------------|---------|---------------|-------|
| Erfassungs-Service | `ingest` | MQTT empfangen, prüfen, speichern oder verwerfen | F01, F02, F06 | MVP, gebaut |
| Überwachungs-Service | `monitor` | Ladestand bewerten, kritische Geräte erkennen, Zustand neu bewerten | F05, F07 | F05 gebaut, Offline (F07) offen |
| Meldungs-Service | `incidents` | Meldung eröffnen, abweisen, schließen | F09 | Meldungen gebaut, Benachrichtigung (E-Mail) offen |
| Einsatz-Service | (noch nicht vorhanden) | Tickets, Zuweisung, Batteriewechsel | Tourenplanung, laut README ausgeschlossen | Future Work |
| Dashboard-Service | `api` und `web` | Lesesichten: Übersicht, Verlauf, Meldungen, Optionen | F03, F04, F13 | MVP, gebaut |

Dazu kommen zwei Pakete ohne eigenen Prozess:

* `shared`: Reine Kernlogik und gemeinsame Typen (Nachrichtenprüfung, Statusbewertung, Ereignisse, API-Typen). Sie wird vom Backend und vom WebAssembly-Frontend verwendet.
* `kit`: Infrastruktur, die jeder Dienst braucht (Konfiguration, Datenbankzugang, NATS, geordnetes Beenden).

### Wie der Schnitt aus einem Monolithen entsteht

Ein modularer Monolith wäre für diese Last ausreichend gewesen. Wer ihn später zerlegen will, geht in dieser Reihenfolge vor. Genau so ist der Schnitt hier entstanden:

1. **Module an den Event-Storming-Grenzen ziehen.** Jeder Service bekommt ein Modul mit klarer Schnittstelle. Kein Modul greift in die Tabellen eines anderen.
2. **Abhängigkeiten durch Ereignisse ersetzen.** Wo ein Modul ein anderes direkt aufrief (`Überwachung.bewerten(messwert)`), tritt ein Ereignis (`ReadingStored`) mit Abonnent.
3. **Datenbesitz klären.** Jede Tabelle gehört genau einem Modul, das als einziges schreibt. Andere dürfen höchstens lesen.
4. **Schemas trennen.** Eine Datenbank-Instanz, ein Schema je Modul, eigene Migrationen. Das ist der letzte Schritt vor einer getrennten Datenbank.
5. **Prozesse trennen.** Jedes Modul bekommt eine eigene `main`-Funktion und ein eigenes Image. Der Bus verbindet die Prozesse.
6. **Idempotenz herstellen.** Ereignisse werden mindestens einmal zugestellt, jeder Abonnent muss Doppelte vertragen.

---

## 4. Zusammenspiel der Dienste

### Ablauf einer Nachricht

1. `ingest` empfängt die MQTT-Nachricht (QoS 1) und prüft sie mit `shared::validation`.
2. Gültig: `ReadingStore` speichert den Messwert, danach veröffentlicht `EventPublisher` das Ereignis `ReadingStored`. Ungültig: Die Nachricht landet mit Grund in `rejected_message`, kein Ereignis.
3. Erst danach bestätigt `ingest` die Nachricht beim Broker. Fällt der Dienst vorher aus, stellt der Broker sie erneut zu.
4. `monitor` bewertet den Ladestand. Wechselt der Zustand, veröffentlicht er `DeviceCritical` oder `DeviceRecovered`.
5. `incidents` eröffnet bei `DeviceCritical` eine Meldung und schließt sie bei `DeviceRecovered`.
6. `api` hört auf alle Ereignisse und schickt sie per Server-Sent Events an die Browser, die daraufhin die betroffenen Daten neu laden.

### Ereignisse und Befehle

| Name | Art | Sender | Empfänger |
|------|-----|--------|-----------|
| `ReadingStored` | Ereignis | `ingest` | `monitor`, `api` |
| `DeviceCritical` | Ereignis | `monitor` | `incidents`, `api` |
| `DeviceRecovered` | Ereignis | `monitor` | `incidents`, `api` |
| `ThresholdChanged` | Ereignis | `monitor` | `api` |
| `IncidentOpened`, `IncidentDismissed`, `IncidentResolved` | Ereignis | `incidents` | `api` |
| `SetThreshold` | Befehl mit Antwort | `api` | `monitor` |
| `DismissIncident` | Befehl mit Antwort | `api` | `incidents` |

Ereignisse laufen über JetStream (gespeichert, jeder Dienst mit eigenem dauerhaften Konsumenten), Befehle über Request/Reply. Der Besitzer eines Datenbestands führt Änderungen aus, die API schreibt selbst nichts.

### Zuverlässigkeit

| Situation | Verhalten |
|-----------|-----------|
| API-Server startet neu | Der Empfang läuft ungestört weiter (NF03), Browser verbinden sich selbst neu |
| `monitor` oder `incidents` ist kurz aus | Die Ereignisse liegen im Stream und werden beim Start nachgeholt |
| Broker-Neustart | `ingest` verbindet sich selbst neu (Wartezeit bis 10 s), die Sitzung bleibt bestehen |
| Datenbank kurz nicht erreichbar | `ingest` wiederholt die Verarbeitung mehrfach und bestätigt die Nachricht nicht, solange sie nicht gespeichert ist |
| Ereignis kommt doppelt an | Gleiche Kennung: NATS verwirft das Duplikat; zusätzlich sind alle Verarbeiter idempotent |
| Ein Abonnent scheitert dauerhaft an einem Ereignis | Wiederholung nach 5 s, höchstens zehnmal |
| Ereignis ist nicht lesbar | Wird verworfen und protokolliert, statt endlos wiederholt |

---

## 5. Datenmodell

Jeder Dienst legt sein Schema selbst an und migriert nur dieses (`sqlx migrate`, Dateien im Ordner `migrations` des Dienstes).

| Schema | Tabelle | Wichtige Felder | Zweck |
|--------|---------|-----------------|-------|
| `ingest` | `device` | `id`, `first_seen_at`, `last_reading_at`, `last_charge_pct` | Legt sich beim ersten Messwert selbst an, schneller Zugriff für die Übersicht |
| `ingest` | `reading` | `device_id`, `ts`, `charge_pct`, `received_at` | Alle gültigen Messwerte (F02), TimescaleDB-Hypertable |
| `ingest` | `reading_hourly` | `device_id`, `bucket`, `charge_pct` | Stundenwerte (Continuous Aggregate) für Monats- und Jahresansicht |
| `ingest` | `rejected_message` | `received_at`, `topic`, `payload`, `reason` | Protokoll verworfener Nachrichten (F06) |
| `monitor` | `setting` | `key`, `value` | Schwellwert (NF07) |
| `monitor` | `device_state` | `device_id`, `charge_pct`, `reading_ts`, `status`, `changed_at` | Zuletzt bewerteter Zustand, Grundlage für die Zustandswechsel |
| `incidents` | `incident` | `id`, `device_id`, `opened_at`, `closed_at`, `state`, `charge_pct` | Meldungen: offen, abgewiesen, erledigt |

Regeln, die die Datenbank absichert: ein Messwert je Gerät und Zeitstempel (Duplikate durch QoS 1 werden ignoriert), höchstens eine offene Meldung je Gerät (partieller Unique-Index), Ladestand nur zwischen 0 und 100.

**Aufbewahrung und Kompression:** Chunks zu 7 Tagen, Kompression nach 7 Tagen (gruppiert nach Gerät), Löschen nach 12 Monaten. Die Dauer von 12 Monaten ist mit dem Auftraggeber noch zu bestätigen (NF08). Das 30-Tage-Diagramm liest rund 720 Stundenwerte statt 8.640 Rohwerte.

**Der Kompromiss beim Lesen:** Die API liest die Tabellen der anderen Schemas nur lesend, statt eine Kopie der Daten zu halten. Eine eigene Kopie von 52 Mio. Messwerten wäre unverhältnismäßig. Der Preis ist eine feste Kopplung an die Tabellenstruktur der anderen Dienste, deshalb liegt die gesamte Lesesicht in einer Datei (`api/src/queries.rs`). Strenger wäre eine eigene Lese-Datenbank je Dienst oder ein Lese-Endpunkt in jedem Dienst.

---

## 6. Technologieentscheidungen

Die ausführlichen Alternativen stehen in der README. Hier die Kurzfassung mit den Gründen, die für diese Architektur wichtig sind.

### Backend: Rust

| Baustein | Wahl | Hinweis |
|----------|------|---------|
| Async-Laufzeit | tokio | Standard, von allen anderen Bausteinen vorausgesetzt |
| MQTT | rumqttc | QoS 1, feste Sitzung, manuelle Bestätigung, selbstständiges Wiederverbinden |
| HTTP und SSE | Axum | Passt zu tokio und tower, SSE ist eingebaut |
| Datenbank | SQLx | Abfragen werden beim Kompilieren gegen die Datenbank geprüft, `.sqlx/` erlaubt Builds ohne Datenbank |
| Bus | async-nats | JetStream für Ereignisse, Request/Reply für Befehle |
| Prüfung | eigene reine Funktionen in `shared` | Kein Framework nötig, dadurch ohne Infrastruktur testbar |

Beim Beobachter-Muster (laut Aufgabenstellung vorgegeben) gibt es zwei Ebenen: Im Empfangsdienst benachrichtigt der `MqttReceiver` über den Trait `ReadingObserver` seine Beobachter `ReadingStore` und `EventPublisher`. Zwischen den Diensten sind die Abonnenten auf dem Bus die Beobachter.

### Frontend: Leptos

Leptos als Single-Page-App (WebAssembly, gebaut mit Trunk), Diagramme mit charming (ECharts), Live-Updates über `EventSource`. Der Vorteil ist der gemeinsame Typ für Backend und Frontend: Ändert sich eine API-Antwort in `shared`, bricht sofort der Frontend-Build. Der Preis sind weniger fertige Komponenten als bei React und ein größerer erster Download. Die Karte (F12) kommt später als Leaflet per JavaScript-Interop.

### Ereignisbus: NATS

| Kandidat | Vorteile | Nachteile |
|----------|----------|-----------|
| **NATS JetStream (gewählt)** | Ein Container, Ereignisse werden gespeichert, Befehle per Request/Reply im selben System, gute Rust-Bibliothek | Ein weiterer Baustein im Betrieb |
| Mosquitto (MQTT intern) | Schon vorhanden | Kein Request/Reply wie benötigt, keine gespeicherten Ereignisse für spätere Abonnenten |
| PostgreSQL LISTEN/NOTIFY | Keine zusätzliche Komponente | Ereignisse gehen verloren, wenn ein Dienst nicht verbunden ist |
| RabbitMQ oder Kafka | Sehr verbreitet | Für diese Größe überdimensioniert |

### Datenbank

PostgreSQL mit TimescaleDB, ein Schema je Dienst. Der Fallback ohne die Erweiterung (native Partitionierung nach Monat) bleibt möglich, kostet aber Eigenbau bei Aggregaten und Löschen. Lizenzfragen von TimescaleDB und die Aufbewahrungsdauer sind vor der Abnahme zu prüfen.

---

## 7. Nicht umgesetzt (Future Work)

| Anforderung | Plan |
|-------------|------|
| F07 Offline-Erkennung | `monitor` bekommt einen Zeitgeber, der Geräte ohne Messwert erkennt und als Status `offline` meldet |
| F08 Prognose | Neuer Abonnent von `ReadingStored` oder Berechnung in `monitor`, reine Funktion in `shared` |
| F09 Benachrichtigung | Neuer Abonnent von `IncidentOpened`, zum Beispiel ein Dienst `notify` mit SMTP |
| F10 Login | Eigene Konten mit Session-Cookie (tower-sessions, Argon2) in der API. NF09 sieht gleichberechtigte Nutzende vor; Rollen wären eine Änderung der Anforderung |
| F11 Export, F12 Karte | Weitere Lesesichten in `api` und `web` |
| Einsatz-Service | Neuer Dienst `jobs` mit eigenem Schema, abonniert die Meldungsereignisse |

---

## 8. Annahmen und offene Fragen

| Frage | Annahme im Code | Wo anpassen |
|-------|-----------------|-------------|
| Nachrichtenformat und Topics (README Frage 1) | Topic `trafficnode/<id>/battery`, JSON mit `device_id`, `timestamp`, `charge_pct` | `shared/src/validation.rs`, Topic per `MQTT_TOPIC` |
| Prozent oder Spannung (Frage 2) | Prozent | `shared/src/validation.rs` |
| Höhe des Schwellwerts (Frage 4) | 20 Prozent, einstellbar | Oberfläche (Optionen) oder `CRITICAL_THRESHOLD_PCT` |
| Aufbewahrung (Frage 5) | 12 Monate | `ingest/migrations/0002_policies.sql` |
| Betrieb auf dem Server | Noch offen: Weiterleitung der Domain durch den Dozenten (Ingress oder Proxy) | `docs/deployment.md` |

Der Betrieb auf dem Schulserver ist noch nicht auf die neue Dienststruktur umgestellt. `deploy/` startet bisher nur einen Platzhalter.
