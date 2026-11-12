# Systemwechsel: vom Privatrechner an den Arbeitsrechner

Diese Anleitung ist für den Moment, in dem du von deinem Privatrechner (mit dem SSH-Zugang zum Server) an den Arbeitsrechner mit VS Code und Claude Code wechselst. Danach gilt: Push, und es wird automatisch gebaut und auf Staging ausgerollt.

## Die Grundidee in drei Sätzen

1. Der Server baut selbst. Am Arbeitsrechner brauchst du weder Rust noch Docker, nur Git, VS Code und Claude Code.
2. Der Arbeitsrechner braucht **keinen** Zugang zum Server. Den Deploy-Schlüssel legst du einmal hier am Privatrechner an, er liegt danach nur in GitHub.
3. Alles, was am Privatrechner noch nicht gepusht ist, existiert am Arbeitsrechner nicht. Darum zuerst pushen.

## Was heute steht

- Es gibt die Fassung 0.1.0 (Tag `v0.1.0`) und danach das Redesign "Cupertino Spatial Telemetry", beides auf dem Branch `feature/mvp-rust`. Bisher nur **lokal committet, nicht auf GitHub**.
- Auf dem Server läuft Staging mit dem neuen Stand (Port 20210, intern). Auf Port 20200 steht die Seite "In Bearbeitung".
- Die Domain `staging.batterybandits.racore.net` ist noch nicht live. Der zentrale Caddy des Dozenten braucht die Einträge `172.22.0.1:20210` (Staging) und `172.22.0.1:20200` (Prod), **nicht** `127.0.0.1`.
- Das automatische Bauen ist geschrieben (`.github/workflows/deploy.yml`, `deploy/setup-github-deploy.ps1`), aber noch nicht eingerichtet.

## Teil 1: Am Privatrechner (jetzt)

1. **Branch hochladen.** Das ist ein eigener Branch, `main` bleibt unberührt, nichts geht kaputt:
   ```bash
   git push -u origin feature/mvp-rust
   git push origin v0.1.0
   ```
2. **Mit Fynn absprechen**, ob mein Stand nach `main` darf. Er ersetzt die alte Fassung. Erst dann einen Pull Request von `feature/mvp-rust` nach `main` öffnen und mergen.
3. **Automatisches Bauen einrichten.** Vorher die GitHub CLI installieren und anmelden:
   ```bash
   winget install GitHub.cli
   gh auth login
   ```
   Dann erst den Probelauf, dann echt:
   ```bash
   powershell -ExecutionPolicy Bypass -File deploy\setup-github-deploy.ps1 -DryRun
   powershell -ExecutionPolicy Bypass -File deploy\setup-github-deploy.ps1
   ```
   Das Skript legt den Deploy-Schlüssel an, trägt ihn eingeschränkt auf dem Server ein, setzt die Secrets in GitHub und löscht den privaten Schlüssel vom Rechner.
   Das Skript startet am Ende einen ersten Deploy, wenn `deploy.yml` auf `main` liegt. Sonst startet der erste Lauf mit dem ersten Push auf `main`.

## Teil 2: Am Arbeitsrechner

1. **Programme installieren:** Git for Windows und Visual Studio Code. Docker nur, wenn du die Seite dort auch lokal starten willst.
2. **Claude Code:** In VS Code im Reiter Erweiterungen "Claude Code" installieren und mit deinem Claude-Konto anmelden.
3. **GitHub-Zugang:** Beim ersten `git push` öffnet sich die Anmeldung im Browser. Alternativ ein Personal Access Token mit dem Recht "repo".
4. **Projekt holen:**
   ```bash
   git clone https://github.com/Noah9935/Battery-Bandits.git
   cd Battery-Bandits
   git checkout feature/mvp-rust      # bis alles in main ist, danach main
   code .
   ```
5. **Claude Code das erste Mal starten** und den Startprompt unten einfügen. Claude Code liest `CLAUDE.md` im Projekt automatisch. Das Gedächtnis dieser Sitzung zieht nicht mit um, darum steht das Wichtigste im Prompt.
6. **Arbeiten und pushen:** Änderungen committen und auf `main` pushen (oder per Pull Request). Danach läuft in GitHub, Reiter **Actions**: erst CI, dann Deploy. Wenn beides grün ist, läuft Staging mit dem neuen Stand.

## Wie du das Ergebnis siehst

- **Im Reiter Actions:** Jeder Lauf mit Protokoll. Das Fazit steht in der Zusammenfassung des Deploy-Laufs.
- **Im Netz:** `https://staging.batterybandits.racore.net`, sobald der Dozent den Caddy-Eintrag (siehe oben) gesetzt hat.
- **Bis dahin:** nur mit einem SSH-Tunnel zum Server, `ssh -N -L 8081:127.0.0.1:20210 bb`, dann http://localhost:8081. Das geht nur an einem Rechner mit Server-Zugang. Für den Arbeitsrechner kannst du später mit dem Skript einen zweiten Schlüssel anlegen lassen oder den Dozenten bitten, den Caddy-Eintrag zu setzen. Dann brauchst du keinen Tunnel mehr.
- **Anmelden:** `noah` oder `fynn`. Die Passwörter stehen nur in `~/battery-bandits/staging/.env` auf dem Server (`ssh bb 'grep BB_USERS ~/battery-bandits/staging/.env'`), nie im Chat und nie im Repository.

## Startprompt für Claude Code am Arbeitsrechner

Diesen Text komplett einfügen:

```
Wir arbeiten an Battery Bandits (DHBW, Software Engineering I): Rust-Dienste, Weboberfläche mit Leptos als WebAssembly, Postgres mit TimescaleDB, Docker. Lies zuerst CLAUDE.md, CHANGELOG.md und docs/Systemwechsel.md.

Stand: Version 0.1.0 plus Redesign "Cupertino Spatial Telemetry" (Bricolage Grotesque und Geist, schwebende Glas-Inseln, View Transitions). Staging läuft auf dem Schulserver, intern auf Port 20210. Pushes auf main werden automatisch gebaut und ausgerollt (GitHub Actions, .github/workflows/deploy.yml).

Regeln:
- Alles auf Deutsch, in Oberflächentexten, Doku und Commit-Nachrichten keine Gedankenstriche und keine Emojis.
- Nur lokal committen. Pushen nur, wenn ich ausdrücklich "push" sage.
- Nie Passwörter, Schlüssel oder Zugangsdaten in den Chat, in Dateien im Repository oder in Logs schreiben.
- Rust läuft nur im Dev-Container: docker compose -f docker-compose.dev.yml run --rm dev <befehl>. Vor jedem Commit: cargo fmt, cargo clippy (nativ und für wasm32-unknown-unknown) und alle Tests, Abdeckung mindestens 92 Prozent.
- Am Server nur Dinge unter unseren Ports 20200 bis 20299 und in unserem Ordner ändern. Den zentralen Caddy des Dozenten nie anfassen.

Nächste Aufgaben:
1. Die Seiten Wartung, Postfach, Optionen und Karte im neuen Design einzeln durchsehen und nachziehen (Desktop und Handy).
2. Prüfen, ob Staging nach einem Push sauber gebaut wurde, und Fehler im Deploy-Lauf beheben.
3. Die Domain staging.batterybandits.racore.net testen, sobald der Dozent den Caddy-Eintrag gesetzt hat.
```

## Wenn etwas nicht klappt

| Problem | Ursache und Lösung |
|---|---|
| Der Deploy-Lauf ist rot mit "Die Secrets fehlen" | Das Setup-Skript wurde noch nicht ausgeführt, oder es lief ohne gesetzte Secrets. Teil 1 Schritt 3 wiederholen. |
| "Permission denied" im Schritt "Zugang vorbereiten" | Der Deploy-Schlüssel ist auf dem Server nicht eingetragen oder wurde ersetzt. Das Skript erneut ausführen, es tauscht Schlüssel und Secret gemeinsam. |
| "Connection timed out" im Deploy | GitHub erreicht Port 22 des Servers nicht. Den Dozenten fragen, oder von Hand `deploy\deploy.ps1 staging` am Privatrechner ausführen. |
| "Auf dem Server fehlt .../.env" | Die Datei `~/battery-bandits/staging/.env` fehlt. Vorlage ist `deploy/.env.staging.example`. |
| Staging antwortet nicht nach dem Deploy | Im Lauf stehen am Ende die Protokolle der Dienste. Meist fehlt in der `.env` ein Wert, oder die Datenbank braucht länger beim ersten Start. |
| Docker startet am Privatrechner nicht | Alte Socket-Dateien. Docker beenden, die Dateien unter `AppData\Local\Docker\run` über WSL löschen, Docker neu starten. |

## Sicherheit in Kürze

- Der Deploy-Schlüssel hat kein Passwort und darf auf dem Server nur Befehle ausführen, kein Terminal öffnen und keine Ports weiterleiten. Er liegt nur in GitHub-Secrets.
- Wer auf `main` pushen darf, kann damit auf dem Server bauen lassen. Zugriff auf das Repository also nur an Fynn und dich geben.
- Der Schlüssel des Privatrechners (`id_ed25519`) wird nie auf einen anderen Rechner kopiert. Jeder Rechner bekommt einen eigenen Schlüssel.
