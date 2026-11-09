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
| `mosquitto`, `test-publisher` | nur lokal (Profil `local`): eigener Broker und Simulator | nein |

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

**Deployen** (Laptop, Repo-Ordner, gewünschter Branch ausgecheckt und committet):

```
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 prod
```

Das Skript überträgt den letzten Commit per `git archive`, baut die Images auf dem Server (cargo-chef, Release) und startet die Umgebung. Der erste Bau dauert mehrere Minuten, spätere profitieren vom Schichten-Cache.

**Datenbank ansehen** (Laptop): `ssh -N -L 20201:127.0.0.1:20201 bb` (Staging: 20211), dann pgAdmin auf `127.0.0.1:20201`.

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
