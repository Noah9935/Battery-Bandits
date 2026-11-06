# Deployment

Zwei Umgebungen auf dem Schulserver (Alias `bb`, Gruppe `tinf25d-battery-bandits`), je mit eigenem Ordner, eigenen Ports und eigener Datenbank. Portbereich der Gruppe: **20200 bis 20299**.

| Umgebung | Branch | Domain | Web (Caddy) | Datenbank | Ordner |
|----------|--------|--------|-------------|-----------|--------|
| staging | `main` | `staging.batterybandits.racore.net` | 20210 | 20211 | `~/battery-bandits/staging` |
| prod | `prod` | `batterybandits.racore.net` | 20200 | 20201 | `~/battery-bandits/prod` |

Alle Ports sind nur an `127.0.0.1` des Servers gebunden. Der Weg von außen ist: Domain, zentraler Caddy des Dozenten (HTTPS), `127.0.0.1:<Web-Port>`, Caddy im Projekt, Dashboard-Service.

**Offen (Dozent):** Der zentrale Caddy kennt die beiden Domains noch nicht (Stand 06.11.2026: kein Zertifikat, HTTPS bricht ab). Er muss `batterybandits.racore.net` auf `127.0.0.1:20200` und `staging.batterybandits.racore.net` auf `127.0.0.1:20210` weiterleiten. Bis dahin ist die Oberfläche auf dem Server mit `curl http://127.0.0.1:20210/` erreichbar.

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

## Lokal ausprobieren

Benötigt nur Docker.

```bash
deploy/start-local.sh          # Linux, macOS, Git Bash
deploy\start-local.ps1         # Windows PowerShell
```

Danach ist das Dashboard unter http://localhost:8080 erreichbar. Der Simulator sendet 12 Knoten rund um Stuttgart und Leonberg (einer kritisch, einer ohne GPS, einer fällt aus, gelegentlich fehlerhafte Nachrichten), die Entladung läuft 60-fach beschleunigt. Prüfung des Durchstichs: `bash tools/smoke.sh http://localhost:8080`.

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

**Deployen** (Laptop, Repo-Ordner, gewünschter Branch ausgecheckt und committet):

```
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 prod
```

Das Skript überträgt den letzten Commit per `git archive`, baut die Images auf dem Server (cargo-chef, Release) und startet die Umgebung. Der erste Bau dauert mehrere Minuten, spätere profitieren vom Schichten-Cache.

**Datenbank ansehen** (Laptop): `ssh -N -L 20201:127.0.0.1:20201 bb` (Staging: 20211), dann pgAdmin auf `127.0.0.1:20201`.

## Entwicklung im Container

`docker-compose.dev.yml` im Repo-Wurzelverzeichnis startet nur Datenbank und Broker und stellt einen Werkzeug-Container bereit:

```bash
docker compose -f docker-compose.dev.yml up -d
docker compose -f docker-compose.dev.yml run --rm dev cargo test --workspace
```
