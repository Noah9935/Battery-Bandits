# Deployment

Zwei Umgebungen auf dem Schulserver (Alias `bb`, Gruppe `tinf25d-battery-bandits`), je mit eigenem Ordner, eigenen Ports und eigener Datenbank. Portbereich der Gruppe: **20200 bis 20299**.

| Umgebung | Branch | Domain | Web (Caddy) | Datenbank | Ordner |
|----------|--------|--------|-------------|-----------|--------|
| staging | `main` | `staging.batterybandits.racore.net` | 20210 | 20211 | `~/battery-bandits/staging` |
| prod | `prod` | `batterybandits.racore.net` | 20200 | 20201 | `~/battery-bandits/prod` |

Die Datenbank ist nur an `127.0.0.1` gebunden, die Weboberfläche an `WEB_BIND` (auf dem Server `0.0.0.0`, siehe unten). Der Weg von außen ist: Domain, zentraler Caddy des Dozenten (HTTPS), `127.0.0.1:<Web-Port>`, Caddy im Projekt, Dashboard-Service.

**Stand 06.11.2026:** Staging läuft auf dem Schulserver (Port 20210), auf Port 20200 steht der Banner "Kommt bald" für die Prod-Domain. Von außen sind die Ports unseres Bereichs durch einen Netzwerkfilter gesperrt, erreichbar ist nur der zentrale Caddy des Dozenten (80 und 443).

**Offen (Dozent):** Der zentrale Caddy (`/opt/ti-labs/Caddyfile`, läuft im Docker-Netz `ti-labs_app`) kennt die beiden Domains noch nicht, HTTPS bricht ab. Er erreicht Ports auf `127.0.0.1` des Servers nicht, deshalb binden wir die Weboberfläche mit `WEB_BIND=0.0.0.0` (die Datenbank bleibt auf Loopback). Der Eintrag im zentralen Caddy lautet:

```
staging.batterybandits.racore.net {
	reverse_proxy 172.22.0.1:20210
}

batterybandits.racore.net {
	reverse_proxy 172.22.0.1:20200
}
```

`172.22.0.1` ist das Gateway des Netzes `ti-labs_app` (der Docker-Host). Getestet: Ein Container erreicht darüber `:20210`, den Datenbank-Port `:20211` nicht. Server-Sent Events gehen durch `reverse_proxy` ohne Zusatz. Bis der Eintrag steht, ist die Oberfläche auf dem Server mit `curl http://127.0.0.1:20210/` erreichbar.

## Bestandteile (`deploy/docker-compose.yml`)

| Dienst | Aufgabe | Von außen erreichbar |
|--------|---------|----------------------|
| `timescaledb` | Datenbank (PostgreSQL mit TimescaleDB), Migrationen laufen beim Start der Dienste | `127.0.0.1:${DB_PORT}` |
| `ingestion` | MQTT empfangen, prüfen, speichern | nein |
| `monitoring` | Status bewerten, Offline erkennen, Prognose | nein |
| `operations` | Meldungen, Batteriewechsel (REST) | nein |
| `dashboard` | Lese-Modelle, Einstellungen, Proxy, Echtzeit (SSE) | nein |
| `caddy` | liefert die Weboberfläche unter `/`, leitet `/api/*` an `dashboard` weiter | `127.0.0.1:${WEB_PORT}` |
| `mosquitto`, `test-publisher` | lokal (Profil `local`) und auf Staging (Profil `sim`): eigener Broker und Simulator | nein |
| `ingestion-sim` | nur Staging (Profil `sim`): nimmt die erfundenen Geräte vom eigenen Broker auf | nein |

Die Dienste sprechen über das interne Netz `backend` miteinander und über Postgres `NOTIFY` und `LISTEN` (Kanal `domain_events`). In Staging und Prod liefert der gemeinsame Broker `mqtt.ti-labs.ai:8883` (TLS) die Daten, Topic `traffic-nodes/+/measurements`. Der Test-Publisher sendet nie dorthin.

Die Oberfläche kennt eigene Adressen (`/geraete/TN0001`). Dafür liefert der Caddy im Web-Image auf jedem Pfad ohne Datei `index.html` aus (`try_files`), und der zentrale Caddy des Dozenten muss nur alles an den Port weiterreichen.

## Lokal ausprobieren

Benötigt nur Docker.

```bash
deploy/start-local.sh          # Linux, macOS, Git Bash
deploy\start-local.ps1         # Windows PowerShell
```

Danach ist das Dashboard unter http://localhost:8080 erreichbar. Der Simulator sendet 12 Knoten rund um Stuttgart und Leonberg (einer kritisch, einer ohne GPS, einer fällt aus, gelegentlich fehlerhafte Nachrichten), die Entladung läuft 60-fach beschleunigt. Prüfung des Durchstichs: `bash tools/smoke.sh http://localhost:8080`.

Die Seite verlangt eine Anmeldung. Lokal gibt es die Übungskonten aus `deploy/.env.local.example` (`noah` und `fynn`, Passwörter dort sichtbar, nur für diese Entwicklungsumgebung).

## Staging und Prod

Ablauf: auf `main` entwickeln, Staging deployen und testen, `main` in `prod` mergen, Prod deployen.

**Einmalig je Umgebung** (auf dem Server) die Datei `.env` anlegen:

```bash
mkdir -p ~/battery-bandits/staging           # bzw. prod
cd ~/battery-bandits/staging
# Vorlage aus dem Repo: deploy/.env.staging.example (bzw. .env.prod.example)
nano .env && chmod 600 .env
```

Darin `POSTGRES_PASSWORD` setzen (`openssl rand -base64 24`) und die Zugangsdaten für den gemeinsamen Broker eintragen (stehen in der vertraulichen YAML der Gruppe, nie ins Repository). `MQTT_CLIENT_ID` muss je Umgebung eindeutig bleiben, weil der Broker mit anderen Gruppen geteilt wird.

**Konten** stehen in `BB_USERS` (`name:passwort:Anzeigename`, getrennt durch Semikolon) und werden beim Start des Dashboard-Service in die Datenbank übernommen. Passwörter werden dort nur als Argon2-Hash gespeichert, die Werte in der `.env` gehören nie ins Repository. Ein geänderter Eintrag setzt das Passwort neu. Ohne Konten startet der Dienst nicht, solange `AUTH_REQUIRED` an ist. Hinter HTTPS zusätzlich `COOKIE_SECURE=true` setzen, sonst nimmt der Browser das Sitzungs-Cookie nicht an. Nach fünf falschen Passwörtern ist ein Name eine Minute gesperrt.

**Deployen** geschieht automatisch: Ein Push auf einen Feature-Branch rollt Staging aus, ein Push auf `main` rollt Prod aus (Abschnitt "Automatisch bauen bei jedem Push"). Ein Skript für den Arbeitsrechner gibt es nicht mehr. Von Hand auf dem Server geht es so (zum Beispiel nach einer Änderung der `.env`):

```bash
ssh bb
cd ~/battery-bandits/prod/src/deploy          # bzw. staging
docker compose --env-file ../../.env up -d    # übernimmt die .env, startet geänderte Dienste neu
```

Mit `build` davor (`docker compose --env-file ../../.env build`) werden auch die Images neu gebaut.

**Datenbank ansehen** (Laptop): `ssh -N -L 20201:127.0.0.1:20201 bb` (Staging: 20211), dann pgAdmin auf `127.0.0.1:20201`.

## Automatisch bauen bei jedem Push (GitHub Actions)

Ablauf: Jeder Push läuft durch das CI (Format, Clippy, Tests, Abdeckung, kompletter Stack mit Durchstich). Ist die Prüfung der Dienste grün, rollt der Job "Ausrollen" in `.github/workflows/ci.yml` den Stand auf dem Schulserver aus:

| Push auf | Ziel | Port | Adresse |
|---|---|---|---|
| ein Feature-Branch (jeder außer `main` und `prod`) | Staging | 20210 | https://staging.batterybandits.racore.net |
| `main` | Prod | 20200 | https://batterybandits.racore.net |

Pull Requests werden nur geprüft, nie ausgerollt. Der Server baut selbst, auf dem Arbeitsrechner braucht es nur Git, VS Code und Claude Code. Das Ergebnis steht im Reiter "Actions", bei Fehlern mit den Protokollen der Dienste.

**Wichtig bei Prod:** Der erste Lauf von `main` beendet die Seite "In Bearbeitung" (sie belegt denselben Port) und startet die App. Das passiert erst nach dem Bauen, die Seite bleibt also, solange gebaut wird. Prod hat eine eigene, leere Datenbank (Port 20201). Staging und Prod senden mit eigener MQTT-Kennung (`battery-bandits-staging`, `battery-bandits-prod`).

**Einmalig eingerichtet** sind ein eigener Deploy-Schlüssel ohne Passwort (auf dem Server in `~/.ssh/authorized_keys`, eingeschränkt auf Befehle, kein Terminal, kein Port-Forwarding) und die GitHub-Secrets `DEPLOY_SSH_KEY`, `DEPLOY_HOST`, `DEPLOY_USER`, `DEPLOY_KNOWN_HOSTS` und optional `DEPLOY_PORT` (Vorgabe 22). Erneuern geht von Hand: neuen Schlüssel mit `ssh-keygen -t ed25519 -N "" -f deploy_key` erzeugen, den öffentlichen Teil auf dem Server eintragen, den privaten Teil mit `gh secret set DEPLOY_SSH_KEY < deploy_key` hinterlegen und die Datei danach löschen.

**Voraussetzungen**
- Auf dem Server liegt je Umgebung eine `.env` (`~/battery-bandits/staging/.env` und `~/battery-bandits/prod/.env`, Vorlagen `deploy/.env.staging.example` und `deploy/.env.prod.example`, mit `BB_USERS`). Fehlt sie, bricht der Lauf mit einer klaren Meldung ab, Zugangsdaten legt der Workflow nie an.
- Das Konto auf GitHub darf Secrets anlegen (Schreibrechte auf das Repository).
- Der Server ist auf Port 22 aus dem Internet erreichbar. Wenn der Dozent das sperrt, geht das Ausrollen nur noch von Hand auf dem Server (siehe oben).

**Von Hand ausrollen:** Reiter "Actions", Workflow "CI", "Run workflow", Branch und Ziel wählen. Das hilft auch, wenn ein Lauf am Netz scheiterte (zum Beispiel beim Download von `wasm-opt`): "Re-run failed jobs" reicht dann meist.

**Arbeiten am Arbeitsrechner**: Repository klonen, in VS Code mit Claude Code arbeiten, committen und auf einen Feature-Branch pushen. Staging zeigt den Stand nach wenigen Minuten. Wer auf `main` pusht oder einen Pull Request mergt, rollt auf Prod aus.

**Gut zu wissen**
- Zwei Pushes kurz hintereinander bauen nacheinander, nie gleichzeitig. Ein Feature-Branch überschreibt den Stand von Staging, auch den eines anderen Branches: Staging zeigt immer den zuletzt gepushten Branch.
- Der erste Lauf dauert mehrere Minuten, spätere nutzen den Docker-Zwischenspeicher.
- Zurück zur Seite "In Bearbeitung": auf dem Server die App beenden (`docker compose --env-file ../../.env down` in `~/battery-bandits/prod/src/deploy`, die Daten bleiben erhalten) und in `deploy/coming-soon` `docker compose --env-file ../../../.env up -d` starten. Der nächste Push auf `main` startet die App wieder.
- Schlüssel erneuern: siehe "Einmalig eingerichtet".

## Konten und Admins

Registrieren kann sich niemand selbst. In `BB_USERS` stehen die Konten, die beim Start angelegt werden (`name:passwort:Anzeigename`, getrennt durch Semikolon). Ein viertes Feld `admin` macht zum Admin: `noah:passwort:Noah Boufercha:admin`. Gibt es noch keinen Admin, wird das erste Konto der Liste einer. Weitere Konten legen Admins in der Oberfläche an (Profil, Zahnrad, Optionen, Konten). Dort lassen sich auch die Rolle vergeben, Passwörter zurücksetzen und Konten löschen. Ein Passwort, das die Person selbst im Profil gesetzt hat, überschreibt `BB_USERS` beim Neustart nicht mehr.

## Weiterleitung per E-Mail einrichten

Im Postfach lässt sich jeder Eintrag mit einem Tipp über WhatsApp oder E-Mail weiterleiten. Das braucht keine Einrichtung. Damit der Server neue Meldungen **selbst** sendet, geht es nur noch per E-Mail: Jede Person trägt im Postfach ihre Adresse ein, und der Zugang zum Mailserver steht einmal für den ganzen Server in der `.env`.

1. Im Mailserver (hier Mailcow, `https://mail.racore.net/admin`) ein Postfach für die Alarme anlegen, zum Beispiel `alarm@racore.net`. Dazu ein App-Passwort erzeugen (Postfach, App-Passwörter, mit SMTP-Recht), damit das Hauptpasswort nicht in der `.env` steht.
2. Auf dem Server (Ordner `~/battery-bandits/staging/src/deploy`) `bash set-smtp.sh` ausführen. Das Skript fragt Mailserver, Postfach und Passwort ab (das Passwort bleibt unsichtbar), kodiert Sonderzeichen, trägt `SMTP_URL` und `SMTP_FROM` in die `.env` ein und startet den Operations-Dienst neu. Das Passwort steht danach nur in der `.env`, nie im Chat, im Repository oder in einem Protokoll.
3. Im Postfach die eigene Adresse eintragen, speichern und "Probenachricht senden" drücken. Das Ergebnis steht dort je Dienst.

Weitere Dienste kann der Server weiterhin senden, die Oberfläche bietet sie fürs automatische Senden aber nicht mehr an. WhatsApp über CallMeBot braucht für jede Nummer einen eigenen Schlüssel, deshalb ist es draußen:

| Dienst | Einstellung | Hinweis |
|---|---|---|
| E-Mail | `SMTP_URL`, `SMTP_FROM` | Zum Beispiel `smtps://alarm%40beispiel.de:passwort@mail.beispiel.de:465`. |
| Telegram | `TELEGRAM_BOT_TOKEN` | Bot bei BotFather anlegen. Die Person schreibt dem Bot einmal und trägt ihre Chat-ID ein. |
| WhatsApp (Gateway) | `WHATSAPP_WEBHOOK_URL` | Adresse eines Gateways (zum Beispiel eines Anbieters der WhatsApp Business API), das `{"channel","to","subject","text"}` als JSON per POST annimmt. |
| WhatsApp (CallMeBot) | nichts auf dem Server | Jede Person schaltet ihre Nummer einmalig frei (callmebot.com). Abschaltbar mit `WHATSAPP_CALLMEBOT=off`, die Adresse ist mit `CALLMEBOT_URL` änderbar. |
| alle | `NOTIFY_WEBHOOK_TOKEN`, `PUBLIC_URL` | Token für die Gateways (als Bearer) und die Adresse der Seite für den Link in der Nachricht. |

Dienste ohne Zugang gelten als "nicht eingerichtet", die Oberfläche zeigt das an. Zugangsdaten stehen nie im Repository und nie in den Protokollen.

## Platz der Datenbank (10 GB)

Die Gruppe hat auf dem Server etwa 10 GB. Deshalb ist der Platz der Datenbank auf **10 GB** begrenzt (`DB_QUOTA_MB`, Vorgabe 10240, in der `.env` der Umgebung änderbar). Zwei Regeln halten die Datenbank darin:

- Die Datenbank löscht Messwerte nach 12 Monaten selbst (Richtlinie aus `migrations/0001_init.sql`) und komprimiert Daten nach 7 Tagen.
- Der Überwachungs-Dienst prüft alle zehn Minuten (`RETENTION_INTERVAL_SECS`), wie groß die Datenbank ist. Liegt sie über **95 Prozent** des Platzes, löscht er die ältesten Datenstücke (Chunks, etwa eine Woche Messwerte) nacheinander, bis es wieder passt. Das jüngste Datenstück bleibt immer. Es steht als Warnung im Protokoll des Dienstes (`docker compose logs monitoring`).

Meldungen, Einsätze, Konten und der Rest bleiben davon unberührt, es verschwinden nur alte Messwerte. Die Seite Effizienz zeigt unter "Datenhaltung" Größe, Füllstand und Prognose. Hat die Festplatte weniger Platz als eingestellt, gilt der echte Platz.

## Echte Daten einspielen

Das Werkzeug `import-history` ist im Image des Ingestion-Service. Es liest JSON-Zeilen `{"t": "<Zeitpunkt>", "topic": "...", "payload": {...}}` von stdin und schickt sie mit dem Originalzeitpunkt durch Prüfung, Ladezustand und Speicherung. `DATA_SOURCE=dozent` (Vorgabe) markiert die Geräte als echt.

```bash
# Historie der Messdatenplattform (nur lesend, Zugangsdaten aus der YAML) direkt in den Importer
psql ... -At -c "select json_build_object('t', received_at, 'topic', topic, 'payload', payload)::text from history.measurements order by received_at" \n  | docker run --rm -i --network <projekt>_backend -e DATABASE_URL=... --entrypoint /usr/local/bin/import-history battery-bandits/ingestion-service:latest
```

Die Vorlesungs-CSV (`traffic_devices_signals.csv`, Gerät `TSN-002`) wird vorher mit einem kleinen Skript in dieses Zeilenformat gebracht (Spannung V in mV, Strom A in mA).

## Entwicklung im Container

`docker-compose.dev.yml` im Repo-Wurzelverzeichnis startet nur Datenbank und Broker und stellt einen Werkzeug-Container bereit:

```bash
docker compose -f docker-compose.dev.yml up -d
docker compose -f docker-compose.dev.yml run --rm dev cargo test --workspace
```

## Erfundene Testgeräte auf Staging

Der Broker der Vorlesung liefert nur wenige echte Geräte. Damit man auf Staging sieht, wie sich die Oberfläche mit einer
größeren Flotte verhält, startet in der `.env` von Staging `COMPOSE_PROFILES=sim` zusätzlich einen eigenen Mosquitto, den
Simulator und einen zweiten Ingestion-Dienst. Echte und erfundene Geräte laufen in derselben Datenbank nebeneinander,
der gemeinsame Broker bleibt unberührt. Auf Prod bleibt `COMPOSE_PROFILES` leer. Die Pipeline bricht das Ausrollen von `main` ab, wenn in der Prod-`.env` der Simulator steht (`COMPOSE_PROFILES` mit `sim` oder `local`, oder `DATA_SOURCE=simulation`). Ohne erfundene Geräte zeigt die Oberfläche keine Herkunftsfilter und keine Marke "Erfunden".

**Der Simulator** (`services/test-publisher`) betreibt 50 feste Nodes, SIM01 bis SIM50, in Stuttgart und Umgebung. Sie
senden so, wie die Nodes der Vorlesung senden: Topic `traffic-nodes/<Kennung>/measurements`, dasselbe JSON (Spannung,
Strom im Entladebetrieb negativ, Temperatur, GPS), 12-V-Batterie mit 80 Ah Nennkapazität, alle zehn Sekunden. Es gibt
keinen Zufall bei den Geräten selbst: Ort, Szenario (normal, Warnung, kritisch, ohne GPS-Fix, verstummt nach einigen
Runden) und Startladung jedes Nodes stehen fest in der Tabelle `NODES` in `services/test-publisher/src/lib.rs`. Die
Entladung läuft in Echtzeit (`SIM_SPEEDUP=1`, 180 bis 440 mA je Node mit Tagesgang bei Verbrauch und Temperatur, eine volle Batterie hält ein bis zwei Wochen), für Vorführungen lässt sie sich beschleunigen. Der Wert in der `.env` auf dem Server gilt. Etwa drei Prozent der
Nachrichten sind absichtlich fehlerhaft (`SIM_INVALID_RATE`), damit das Protokoll der verworfenen Nachrichten etwas zeigt.

**Woran man erfundene Nodes erkennt:** nur an der Kennung SIMnn und der Herkunft `simulation`, die der zweite
Ingestion-Dienst setzt (`DATA_SOURCE=simulation`). Die Oberfläche zeigt sie als "Erfunden". Sonst ist alles gleich, auch der
Weg: MQTT-Broker, Ingestion, Datenbank.

**Batterien:** Migration 0020 legt fünf 12-V-AGM-Batterien der Klasse 80 bis 100 Ah mit Händlerpreisen (Stand November
2026) im Katalog an. Der Operations-Dienst baut sie den erfundenen Geräten fest ein (SIM01 das erste Modell, SIM02 das
zweite und so fort), sobald ein Gerät zum ersten Mal sendet. Dadurch haben Preis pro Tag und der Vergleich der Modelle in
der Effizienz echte Zahlen. Kein Hersteller nennt diese Typen ausdrücklich für Ampeln, es sind die gängigen Typen der
Klasse. Preise und Einträge lassen sich in der Oberfläche ändern.

**Den Simulator durch die Nodes der Vorlesung ersetzen:** Das Profil `sim` weglassen (`COMPOSE_PROFILES` leer). Dann läuft
nur der normale Ingestion-Dienst gegen den Broker der Vorlesung (`MQTT_HOST`, `MQTT_PORT`, `MQTT_USER`,
`MQTT_PASSWORD`, `MQTT_TOPIC_FILTER`), und alles, was von dort kommt, trägt die Herkunft `dozent`. Am Format ändert sich
nichts, es ist kein Code anzupassen. Die erfundenen Geräte lassen sich danach in der Datenbank entfernen
(`delete from devices where data_source = 'simulation';`, Messwerte, Meldungen und Einsätze verschwinden mit) oder
bleiben als gekennzeichnete Testgeräte stehen.
