# Deployment

Zwei Umgebungen auf dem VPS, je mit eigenem Ordner, eigenen Ports und eigener DB:

| Umgebung | Branch | Domain | Web | DB | Ordner |
|----------|--------|--------|-----|----|--------|
| staging | `main` | `staging.batterybandits.racore.net` | 20210 | 20211 | `~/battery-bandits/staging` |
| prod | `prod` | `batterybandits.racore.net` | 20200 | 20201 | `~/battery-bandits/prod` |

Domain → Proxy des Dozenten (HTTPS) → `127.0.0.1:<Web-Port>` → Caddy. Konfiguration in [`deploy/`](../deploy/).

**Ablauf:** auf `main` entwickeln → staging deployen und testen → `main` in `prod` mergen → prod deployen.

**Deployen** (Laptop, Repo-Ordner, gewünschten Branch ausgecheckt):
```
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 staging
powershell -ExecutionPolicy Bypass -File deploy\deploy.ps1 prod
```

**Einmalig pro Umgebung `.env` anlegen** (Server, nach dem ersten Deploy-Versuch):
```bash
cd ~/battery-bandits/staging          # bzw. prod
cp .env.example .env && chmod 600 .env && nano .env
```
Projektname und Ports laut Tabelle setzen, `POSTGRES_PASSWORD` mit `openssl rand -base64 24` erzeugen.

**Übergang ohne Docker:** `deploy/www` läuft per `python3 -m http.server 20200` (Autostart per `crontab @reboot`) unter http://batterybandits.racore.net:20200. Update: `scp -r deploy/www bb:~/battery-bandits/prod/`. Vor dem ersten Docker-Deploy beenden: `ssh bb "crontab -r; pkill -f 'http.server 20200'"`.

**DB-Tunnel** (Laptop): `ssh -N -L 20201:127.0.0.1:20201 bb` (staging: 20211), dann pgAdmin auf `127.0.0.1:20201`.
