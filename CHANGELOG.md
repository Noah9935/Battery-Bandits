# Änderungsprotokoll

## 0.2.0, für das Handy gebaut (November 2026, in Arbeit)

Die Oberfläche ist jetzt zuerst für das Handy im Einhandbetrieb gedacht.

**Übersicht und Navigation**
- Der große Kopfbereich ist durch eine Dringlichkeits-Anzeige ersetzt: kritische, ausgefallene und auffällige Geräte laufen als Karten durch (von selbst oder per Wischen). Jede Karte zeigt, wie lange das Gerät schon in diesem Zustand ist, zum Beispiel "Offline seit 2 Std. 14 Min.". Der Zeitpunkt des Statuswechsels steht in der Datenbank (`status_changed_at`).
- Schneller Seitenwechsel: Besuchte Seiten bleiben im Speicher, Daten werden nur bei Änderung neu gezeichnet, die Anfragen laufen gleichzeitig, die Karte baut sich im nächsten Bild auf, die Unschärfe über scrollendem Inhalt ist entfernt.
- Zwischen den sechs Seiten (Übersicht, Karte, Wartung, Postfach, Effizienz, Optionen) wechselt man auch durch Wischen. Auf der Karte, im Diagramm und in Reglern ist das Wischen abgeschaltet.
- Fehlermeldungen liegen über dem Inhalt und lassen sich nach links, rechts oder oben wegwischen. Solange das Problem besteht, zeigt die Kopfzeile statt "Live" ein rotes Warndreieck, ein Tipp zeigt die Einzelheiten.
- Die Karte bleibt frei: schmale Suchleiste oben, einklappbares Blatt unten, keine Unschärfe. Tippflächen sind mindestens 48 Pixel groß.

**Wartung**
- Batteriewechsel mit zwei Tipps: Knopf auf der Gerätekarte, in der Tabelle, in der Merkliste, auf der Geräteseite und im Reiter Wartung, dann "Speichern". Gerät, Zeitpunkt und voller Ladestand sind vorbelegt, Techniker und Notiz optional. Ein geplanter Einsatz wird abgeschlossen, sonst entsteht ein erledigter. Der alte Zyklus wird für den Vergleich festgehalten.
- Geräte anpinnen ("Merken") in Übersicht, Karte und Gerät. Die Merkliste berechnet vom eigenen Standort (Browser, sonst Standardstandort aus den Optionen) die kürzeste Reihenfolge (bis acht Halte exakt) und öffnet sie als mehrstufige Route in Google Maps.

**Postfach**
- Jeder Eintrag lässt sich mit einem Tipp über WhatsApp, Telegram, SMS oder E-Mail weiterleiten (vorbereiteter Text mit Link zum Gerät).
- Regel je Person für die automatische Weiterleitung neuer Meldungen, optional nur kritische. Der Server sendet sie, sobald die Zugänge in seiner Umgebung stehen (`TELEGRAM_BOT_TOKEN`, `WHATSAPP_WEBHOOK_URL`, `SMS_WEBHOOK_URL`, `SMTP_URL`, `SMTP_FROM`, `PUBLIC_URL`). Probenachricht und Protokoll inklusive.

**Effizienz**
- Neue Seite und neue Sektion im Gerät: mittlerer Strom, Spannungsabfall pro Tag, reale gegen rechnerische Laufzeit bei 80 Ah, Effizienz in Prozent, Spannung nach Temperaturbereich, Spannung unter hoher Last und Vergleich mit früheren Batteriezyklen. Dazu der Datenbestand (Zahl der Messwerte, Zeitraum, verworfene Nachrichten).

**Grenzen**
- Die Effizienz ist eine Schätzung (Spannung fällt zwischen voll und leer linear). Ein Einfluss des Standorts ist nicht ausgewertet, nur die gemessene Temperatur.
- WhatsApp und SMS brauchen ein Gateway eines Anbieters, E-Mail einen SMTP-Zugang. Ohne sie bleibt das Weiterleiten per Tipp.
- Die Google-Maps-Route zählt höchstens elf Punkte, die Reihenfolge bei mehr als acht Halten ist eine Näherung.

## 0.1.0, erste Version (November 2026)

Battery Bandits überwacht die Batterien von TrafficNodes (12 Volt, 80 Ah), erkennt rechtzeitig, welche Geräte leer werden oder ausfallen, und organisiert die Wartung im Team.

### Was das System kann

**Messwerte empfangen und auswerten**
- Empfang der Messwerte per MQTT (Broker `mqtt.ti-labs.ai`, Topic `traffic-nodes/+/measurements`, verschlüsselt). Fehlerhafte Nachrichten werden geprüft und abgewiesen, die Temperatur ist optional.
- Ladezustand aus der Stromintegration (Trapezregel, 80 Ah), Startwert aus der Ruhespannung geschätzt.
- Status je Gerät: in Ordnung, Warnung (unter 40 Prozent), kritisch (unter 20 Prozent), offline (15 Minuten ohne Messwert). Alle Schwellen sind in den Optionen einstellbar.
- Prognose der Restlaufzeit aus der Entladung der letzten 24 Stunden ("in 3 Std. leer", "in 9 Std. kritisch").
- Jedes Gerät ist als "vom Dozenten" oder "erfunden" (Simulator) gekennzeichnet. Importiert sind die echte Historie von TN0001 und die Vorlesungsdaten.

**Meldungen**
- Genau eine offene Meldung je Gerät: Ladestand kritisch, keine Messwerte mehr oder Ausfall trotz hohem Ladestand.
- Auffälligkeiten, die der Ladezustand nicht zeigt: Messwerte bleiben stundenlang unverändert (Sensor hängt) und plötzlicher Spannungseinbruch. Schwellen in den Optionen einstellbar.
- Meldungen schließen sich bei Erholung, abgewiesene Meldungen bleiben 24 Stunden ruhig.

**Einsätze und Zusammenarbeit**
- Einsätze als Tickets: Art (Batteriewechsel, Kontrolle, Reparatur), Dringlichkeit, Frist, zuständige Person und Kommentare.
- Filter Alle, Meine, Ohne Zuständigkeit sowie "Ich kümmere mich darum". Ein bestätigter Batteriewechsel startet einen neuen Entladezyklus.
- Postfach je Person für neue Meldungen, Zuweisungen und Kommentare, mit Zähler und "Alle als gelesen markieren".
- Protokoll: wer hat wann was getan, gesamt und je Gerät.
- Tour: Geräte mit Handlungsbedarf in der Reihenfolge des nächstgelegenen Halts (Luftlinie), optional vom eigenen Standort, mit Route in Google Maps. Auf dem Handy mit großem "Aktiver Halt".

**Oberfläche**
- Fünf Seiten: Übersicht, Standortkarte, Wartung (Einsätze, Tour, Protokoll), Postfach, Optionen.
- Eigene, teilbare Seite je Gerät unter `/geraete/TN0001`, dazu ein Slide-Over mit Telemetrie, Diagramm (24 Stunden bis 30 Tage mit Warn- und Kritisch-Linie), Einsatz und Protokoll.
- Live-Aktualisierung ohne Neuladen (Server-Sent Events), Suche, Filter, Sortierung, Favoriten, CSV-Export.
- Design im Stil moderner Apple-Software: eine Systemschrift mit tabellarischen Ziffern, segmentierte Navigation, Lade-Ringe, Pillen für Status, dezente Animationen (bei reduzierter Bewegung abgeschaltet). Hell, dunkel, automatisch, hoher Kontrast und Kompakt. Auf dem Handy untere Navigationsleiste.
- Befehlspalette (Strg+K), Tastenkürzel (1 bis 5 für die Seiten, T Farbschema), zwölf einzeln abschaltbare Zusatzfunktionen im Labor.

**Konten und Sicherheit**
- Anmeldung ist Pflicht. Konten kommen aus `BB_USERS` der Server-`.env`, Passwörter liegen nur als Argon2-Hash in der Datenbank, die Sitzung ist ein HttpOnly-Cookie.
- Nach fünf falschen Passwörtern ist ein Name eine Minute gesperrt. Der Name im Protokoll stammt immer aus der Sitzung auf dem Server.

### Technik
- Rust (Edition 2024): vier Dienste (Erfassung, Überwachung, Meldungen und Einsatz, Dashboard) und ein Testsender, Weboberfläche mit Leptos 0.8 als WebAssembly.
- Postgres mit TimescaleDB (acht Migrationen), Ereignisse über `NOTIFY` und `LISTEN`, alles in Docker. Deployment per `deploy/deploy.ps1`, lokal per `deploy/start-local.sh`.
- Prüfung: über 320 Tests, Clippy nativ und für WebAssembly, Abdeckung etwa 95 Prozent, CI in GitHub Actions.

### Bekannte Grenzen
- Auf dem Schulserver läuft noch eine ältere Fassung ohne Anmeldung. Ausrollen braucht Passwörter in der Server-`.env`.
- Die Domain steht erst, wenn der Dozent seinen zentralen Caddy einrichtet.
- Das BatteryTrack-Topic für Batteriewechsel ist unbekannt, Wechsel werden bis dahin nur in der Oberfläche bestätigt.
- Keine E-Mail-Benachrichtigung, keine Rollen und keine Oberfläche zum Anlegen von Konten.
- Schwellen für Auffälligkeiten, Vorzeichen des Stroms und Spannungsgrenzen sind Annahmen, die mit dem Auftraggeber zu klären sind.
