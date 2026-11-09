# Änderungsprotokoll

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
