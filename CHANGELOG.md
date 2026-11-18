# Änderungsprotokoll

## 0.2.0, für das Handy gebaut (November 2026, in Arbeit)

Die Oberfläche ist jetzt zuerst für das Handy im Einhandbetrieb gedacht.

**Nachbesserungen (18.11.2026)**

*Verlauf mit Lücken und freiem Zeitraum*
- War ein Gerät eine Zeit lang nicht aktiv, reißt die Kurve dort ab: Zwischen zwei Messpunkten, die weiter als das Dreifache des üblichen Abstands auseinanderliegen (mindestens 15 Minuten), gibt es weder Linie noch Fläche noch Fehlerband und auch keine verbindende Gerade. Ein Messwert, der zwischen zwei Lücken allein steht, erscheint als Punkt. Bei weniger als drei Messwerten gibt es keine Lücke.
- Neben 24 Std., 7 Tage, 30 Tage und Gesamt gibt es "Datum": zwei Datumsfelder, Von und Bis, beide Tage zählen mit (Ortszeit des Browsers, Vorbelegung die letzten sieben Tage). Die Schnittstelle nimmt dafür `range=custom&from=...&to=...` (UTC mit `Z`, `from` eingeschlossen, `to` ausgeschlossen); fehlen die Zeiten oder liegt der Anfang nicht vor dem Ende, antwortet sie mit 422 und einem deutschen Text. Die Kurve hat wie bei Gesamt höchstens rund 360 Punkte, die Fensterbreite richtet sich nach der Länge des Zeitraums. Eine Prognose gibt es nur, wenn der Zeitraum bis jetzt reicht. Der CSV-Export folgt dem Zeitraum (Datei `TN0002_2026-11-01_2026-11-03.csv`).

*WhatsApp und Telegram entfernt, es bleibt die E-Mail*
- Weitergeleitet wird nur noch per E-Mail. Der Tipp auf "Weiterleiten" an einem Eintrag öffnet direkt das Mailprogramm mit vorbereitetem Text (ein Knopf statt eines Menüs), das Teilen-Fenster eines Geräts bietet E-Mail, "Senden an ..." (Teilen-Fenster des Telefons) und Kopieren. Die Regel im Postfach hat nur noch Hauptschalter, "Nur kritische" und die Adresse, die Probenachricht heißt "Probemail" und zeigt ihr Ergebnis in einer Zeile.
- Aus dem Code ist alles verschwunden, was nur für die anderen Dienste da war: der Kanal-Typ, die Versandwege über Telegram, das WhatsApp-Gateway und CallMeBot, die Prüfung von Handynummern und Chat-IDs, der WhatsApp-Text mit Sternchen, der Kurztext für Messenger, die Symbole "message-circle" und "send", die Abhängigkeit `reqwest` im Operations-Dienst sowie `TELEGRAM_BOT_TOKEN`, `WHATSAPP_WEBHOOK_URL`, `NOTIFY_WEBHOOK_TOKEN`, `WHATSAPP_CALLMEBOT` und `CALLMEBOT_URL` (Compose, `.env`-Beispiele, Doku). Übrig bleiben `SMTP_URL`, `SMTP_FROM` und `PUBLIC_URL`.
- Schnittstelle: `GET /api/notifications/status` (statt `/channels`) liefert `{"ready": true}`, `POST /api/notifications/test` ein einzelnes Ergebnis `{"ok", "detail"}` statt einer Liste, die Regel besteht aus `enabled`, `only_critical` und `email_address`.
- Migration 0024 löscht die Spalten für WhatsApp, Telegram und den eigenen E-Mail-Schalter aus den Regeln und die Spalte `channel` aus dem Versandprotokoll (samt der Einträge für die anderen Dienste). Wer die automatische Weiterleitung nur über WhatsApp oder Telegram eingeschaltet hatte, bekommt nichts mehr, bis er sie mit einer Adresse neu einschaltet.
- Die Tests des Versands laufen gegen einen kleinen nachgestellten Mailserver (SMTP) statt gegen nachgestellte HTTP-Dienste.

*Fehlerspanne in Prozent*
- Die Fehlerspanne des Ladezustands steht in Prozent statt in "Punkten": "69 %" mit "± 6 %" (Oberfläche, E-Mail und der Kurztext der Meldung "Ladestand 12 % (± 3 %)"). Die Entladerate in der Erklärung zur Prognose heißt "% pro Stunde".

*Katalog und Befehlspalette aufgeräumt*
- Im Batterie-Katalog (Wartung, Batterien) stehen je Zeile nur noch Name und Preis. Die Notiz klappt unter der Zeile auf, wenn man mit der Maus darüberfährt, die Zeile per Tastatur anspringt oder sie am Handy antippt.
- Die Befehlspalette (Strg+K) zeigt bei leerer Suche nur die fünf Hauptseiten und kommt so ohne Bildlaufleiste aus, beim Tippen höchstens acht Treffer. Dafür öffnet sie jetzt fast alles: die Reiter der Wartung (Einsätze, Merkliste, Tour, Batterie-Katalog, Wartungsprotokoll), Abschnitte der Seiten (Offene Meldungen, Datenhaltung, Schwellwerte, Auffälligkeiten, Standardstandort, Papierkorb, Labor, Profilbild, Passwort ändern und weitere), alle Farbschemata, die Filter nach Status, Abmelden, die Kontenverwaltung (nur für Admins) und die CSV-Exporte. Ein Abschnitt wird geöffnet und ins Bild gescrollt. Der Wartungs-Reiter liegt jetzt im Store, damit die Palette ihn direkt wählen kann.

**Nachbesserungen (17.11.2026)**

*Gesamtverlauf im Verlauf eines Geräts*
- Neben 24 Std., 7 Tage und 30 Tage gibt es "Gesamt" (`?range=all`): alles, was von dem Gerät gespeichert ist, vom ersten Messwert an, auch über Batteriewechsel hinweg. Die Kurve hat dabei höchstens rund 360 Punkte: Die Fensterbreite wächst mit der Länge der Aufzeichnung (mindestens 5 Minuten, bei Gerät 101 mit rund 14 Tagen eine Stunde, ab einem Jahr ganze Tage), jedes Fenster ist der Mittelwert seiner Messwerte.
- Die Zeitachse passt sich an: bis vier Tage Datum mit Uhrzeit, danach das Datum, ab gut einem Jahr mit Jahr. Die Prognose ragt höchstens ein Viertel der Aufzeichnung in die Zukunft, die Fehlerfläche um die Kurve bleibt.
- Der CSV-Export kennt `range=all` (Datei `TN0001_all.csv`), gilt wie bisher bis höchstens 200000 Zeilen. Gespeichert ist nur, was die Überwachung bei knappem Platz nicht schon gelöscht hat.

*Ladezustand an echten Reihen geprüft und nachgestellt* (Einzelheiten in `docs/ladezustand.md`)
- Das Rauschen der Spannung ist jetzt gemessen statt geschätzt: rund 3 mV an Gerät 101, im Modell 5 mV (vorher 15 mV).
- Die erste Messung eines Geräts gilt nicht mehr als sicher: Bei TN0001 lag sie 6 Punkte über allen späteren (frisch geladene Batterie). Das Filter startet mit 4 Punkten Zuschlag, die Unsicherheit eines Startwerts aus der Spannung ist auf 30 Punkte begrenzt.
- Neu: Selbstkontrolle des Filters (`fit`, gleitender Mittelwert der normierten Abweichungen). Passt eine Batterie nicht zum Modell, wird ihre Fehlerspanne von selbst breiter angegeben, nie schmaler. Über 3 gibt es eine Warnung im Protokoll des Erfassungs-Dienstes.
- Abspielwerkzeug für echte Reihen: `crates/shared/tests/replay_real.rs` (mit `REPLAY_CSV`, nicht Teil des normalen Testlaufs). Ergebnis auf Gerät 101 und TN0001: keine Abweisung, kein Neustart, Selbstkontrolle 0,03 und 0,12 (Ziel höchstens 1), die Annahmen sind dort eher zu vorsichtig. Die simulierte Welt der Tests rauscht jetzt mit dem Doppelten des gemessenen Werts, die Abdeckung der 95-%-Spanne bleibt bei 98 bis 100 Prozent.
- `serde_json` rechnet mit `float_roundtrip`: Der gespeicherte Filterzustand kommt bitgleich aus der Datenbank zurück.

*SMS entfernt*
- Es gibt keinen SMS-Kanal mehr: kein Dienst `Sms`, keine Felder `sms` und `sms_number` in der Regel, kein `SMS_WEBHOOK_URL` (Compose, `.env`-Beispiele, Dokumentation). Migration 0023 löscht die Spalten, die alten SMS-Einträge im Protokoll und erlaubt den Kanal nicht mehr. Übrig bleiben WhatsApp, Telegram und E-Mail.

**Nachbesserungen (16.11.2026)**

*Ladezustand und Prognose mit Fehlerangabe* (Einzelheiten und Prüfung in `docs/ladezustand.md`, Migration 0022)
- Der Ladezustand wird mit einem Kalman-Filter geschätzt, das Ladungsintegration (Vorlesung) und Ruhespannung zusammenführt. Es lernt den Offset des Stromsensors und die Abweichung der Kapazität selbst und gibt zu jedem Wert eine Standardabweichung an. Die Oberfläche zeigt "72 %" mit "± 6 Punkte" (95 Prozent Wahrscheinlichkeit), im Verlauf liegt die Fehlerspanne als Fläche um die Kurve.
- Datenlücken erfinden keine Entladung mehr: Nach einer Pause von drei Tagen bei 500 mA hatte die alte Rechnung etwa 45 Prozentpunkte abgezogen und das Gerät als fast leer gemeldet. Jetzt wächst nur die Unsicherheit, und die nächste Spannungsmessung holt den Wert zurück. Ausreißer in der Spannung werden abgewiesen, ein falscher Zustand (zum Beispiel ungemeldeter Batteriewechsel) wird nach zwölf Abweisungen aus der Spannung neu aufgebaut.
- Die Spannungskurve ist gemessen statt angenommen: Sie stammt aus einer vollständigen Entladung von Gerät 101 (14 Tage, 12,7 V bis 3,9 V). Die frühere Gerade zwischen 10,5 und 12,7 V überschätzte mittlere Ladestände um 10 bis 17 Punkte (TN0001: 72,7 Prozent statt 82). Der Simulator folgt derselben Kurve, samt Temperatur und Spannungsabfall.
- Die Restlaufzeit hat eine Spanne ("Leer in ca. 25 Std. (18 bis 36 Std.)", im Verlauf als Trichter). Sie folgt aus der Fehlerfortpflanzung von Ladezustand und Entladerate und gilt mit etwa 80 Prozent Wahrscheinlichkeit.
- Warnung und kritisch entscheiden mit dem vorsichtigen Wert (Schätzung minus eine Standardabweichung): Je unsicherer der Ladezustand, desto früher wird gewarnt.
- Auch die Meldungen nennen die Spanne: Die E-Mail zeigt "± 3 Punkte" beim Ladestand und "etwa 14 bis 31 Std." bei der Restlaufzeit, der Kurztext der Meldung "Ladestand 12 % (± 3 %)". Die Unsicherheit eines Startwerts aus der Spannung ist auf 30 Punkte begrenzt (mehr als bei reinem Raten wäre ohne Aussage). Die letzte Zeitbeschriftung im Verlauf wird nicht mehr am rechten Rand abgeschnitten.
- Geprüft an einer simulierten Welt mit bekannter Wahrheit (80 Batterien je Fall, mit Datenlücken und stark abweichenden Kennlinien): Die 95-Prozent-Spanne deckt 98 bis 100 Prozent der Fälle ab, der mittlere Fehler liegt bei 1,5 bis 3 Punkten (altes Verfahren 7 bis 8). Auf den echten Reihen ist es ein Konsistenztest, weil die Kurve aus Gerät 101 stammt.
- Neu in der Datenbank: `devices.soc_sigma_percent`, `devices.estimator_state` (Zustand des Filters, überlebt Neustarts), `devices.forecast_hours_low` und `forecast_hours_high`, `readings.soc_sigma_percent`. Beim ersten Messwert nach dem Ausrollen beginnt das Filter bei jedem Gerät neu aus der Spannung und ersetzt den bisherigen Wert.

*Nur echte Daten auf Prod*
- Ohne erfundene Geräte (Prod) fallen Herkunftsfilter, "Dozent"-Beschriftungen, die Marke "Erfunden", die Kartenlegende zur Herkunft und die Herkunftszeile im Teilen-Text ganz weg. Sie erscheinen nur dort, wo der Simulator läuft (Staging, lokal). Ein alter Herkunftsfilter aus einem geteilten Link macht die Liste dann nicht mehr leer.
- Das Ausrollen von `main` bricht ab, wenn in der `.env` von Prod der Simulator steht (`COMPOSE_PROFILES` mit `sim` oder `local`, oder `DATA_SOURCE=simulation`).

*Aufgeräumter*
- Erklärende Texte stehen hinter einem kleinen Fragezeichen (Meldungen, Karten, Prognose, Postfach, Nodes, Profil), andere sind ganz weg (Protokoll, Batteriekatalog, Meine Einsätze, Start-Zeilen der Route, Standort-Hinweis).
- Die Meldungskarten oben haben keine eigene Zeile mit Punkt und Zustand mehr, der farbige Streifen und "seit ..." genügen. Die Schwelle steht hinter einem Fragezeichen.
- Die Filter der Geräteliste stecken hinter einem Knopf "Filter" mit Zähler. "Kritische zuerst" ist weg, sortiert wird über Auswahl und Pfeil.
- "Zu tun" zeigt drei Meldungen pro Seite, "Zuletzt erledigt" drei Einsätze, der Batteriekatalog fünf (ab der sechsten Batterie gibt es Seiten).
- Die Trefferliste bei der Personensuche legt sich über den Inhalt, statt ihn nach unten zu schieben. Der Knopf "Schnellste Route starten" ist weicher, "Neue Batterie anlegen" größer, das Symbol des Batteriewechsels sind zwei Pfeile im Kreis.

*Handy*
- Die Plus- und Minusknöpfe der Karte sind am Handy weg (Zoomen mit zwei Fingern). Der Name "Battery Bandits" steht auch bei schmalen Bildschirmen neben dem Logo.
- Das Wischen der Meldungskarten ist ruhiger: Der Abstand zwischen den Karten wird beim Rasten mitgerechnet, jede Karte rastet einzeln ein, und das automatische Weiterlaufen pausiert nach einer Berührung.

**Nachbesserungen (15.11.2026)**

*Simulator und Daten*
- Der Simulator ist jetzt ein MQTT-Broker mit 50 festen Nodes (SIM01 bis SIM50) rund um Stuttgart, Leonberg und die Filder. Sie senden exakt wie die Nodes der Vorlesung (Topic `traffic-nodes/<Kennung>/measurements`, dasselbe JSON, Strom im Entladebetrieb negativ) und unterscheiden sich nur durch die Kennung SIMnn und die Herkunft `simulation` ("Erfunden" in der Oberfläche). Ort, Szenario und Startladung jedes Nodes stehen fest in einer Tabelle, es gibt keine Zufallsgeräte mehr (vorher 12 feste und beliebig viele zufällige bis 500). Fünf Nodes starten kritisch, rund sechs mit Warnung, zwei haben nie GPS, vier verstummen nach einigen Runden. Die Entladung läuft auf Staging in Echtzeit (`SIM_SPEEDUP=1`), lokal bleibt sie beschleunigt. Auf die Nodes der Vorlesung wechselt man, indem man das Profil `sim` weglässt, siehe `docs/deployment.md`.
- Die erfundenen Beispiel-Einsätze mit Kommentaren sind weg (`seed_demo_tasks` entfernt). Migration 0021 räumt einmalig alles auf, was für erfundene Geräte vorher entstanden ist (Geräte bis SIM60 mit Messwerten, Meldungen, Einsätzen, Kommentaren, Merklisten, Touren, Postfach und Protokoll), der Simulator legt SIM01 bis SIM50 mit der nächsten Nachricht neu an. Echte Geräte bleiben unberührt, auf Prod ändert sich nichts.
- Der Batteriekatalog ist vorbelegt (Migration 0020): fünf 12-V-AGM-Batterien der Klasse 80 bis 100 Ah mit Händlerpreisen, Stand November 2026 (Effekta BTL 12-80 für 151,18 Euro, BTL 12-100 für 175,63 Euro, Victron AGM Deep Cycle 90 Ah für 212,62 Euro, Dometic NDS GreenPower GP80 für 255,00 Euro, Electronicx AGM Marine 80 Ah für 115,99 Euro). Kein Hersteller nennt diese Typen ausdrücklich für Ampeln, es sind die gängigen der Klasse. Der Operations-Dienst baut sie den erfundenen Geräten fest ein (SIM01 das erste Modell, SIM02 das zweite, SIM06 wieder das erste), damit Preis pro Tag und der Modellvergleich in der Effizienz echte Zahlen haben.
- Der Platz der Datenbank ist auf 10 GB begrenzt (`DB_QUOTA_MB`, Vorgabe 10240, so viel hat die Gruppe auf dem Server). Ab 95 Prozent löscht der Überwachungs-Dienst alle zehn Minuten die ältesten Datenstücke der Messwerte (nacheinander, das jüngste bleibt immer), zusätzlich zur Aufbewahrung von 12 Monaten. Hat die Festplatte weniger Platz, gilt der echte Platz (`statvfs`). "Datenhaltung" nennt das in der Prognose. Alles andere (Meldungen, Einsätze, Konten) bleibt unberührt. Siehe `docs/deployment.md`.
- Die Entladung des Simulators ist näher an echten Nodes: 180 bis 440 mA mittlerer Strom je Node (eine volle 80-Ah-Batterie hält in Echtzeit ein bis zwei Wochen, eine fast leere ein bis zwei Tage), tagsüber etwas mehr Verbrauch als nachts, die Temperatur folgt einem Tagesgang statt einer Schwingung im Zehnminutentakt. Die Uhr der Simulation läuft mit `SIM_SPEEDUP` mit. Wichtig: Der Wert in der `.env` auf dem Server gilt, dort muss `SIM_SPEEDUP=1` stehen (bei 10 sind die Batterien nach Stunden leer).

*Oberfläche*
- Der Papierkorb in den Optionen ist knapper: nur "Der Papierkorb ist leer." und "Wird automatisch nach 30 Tagen geleert." Er nimmt jetzt auch aus dem Katalog entfernte Batterien auf (30 Tage, zurückholen oder endgültig löschen), nicht nur Touren. Ausgemusterte Geräte stehen weiter dort.
- Karte: Die Vorgabe ist Schwarz-Weiß, danach die farbige Karte (auch im dunklen Farbschema farbig), dann Satellit. Am Handy steckt der Kartenstil hinter einem Ebenen-Knopf mit Menü und braucht keine eigene Zeile mehr.
- Die Geräteliste passt ihre Höhe an die Zahl der Einträge an. Vorher blieb die Karte nach dem Filtern so hoch wie die volle Liste (leerer Block unter zwei Geräten), weil die Höhe gegen Springen festgehalten und nie freigegeben wurde. Jetzt bleibt sie nur so lange fest, bis die Seite an ihrer Stelle steht.
- Kopfzeile am Handy: Lupe als runder Knopf, gleiche Abstände zu Verbindungspunkt und Profilbild.
- Nach einem Mausklick und einem Tastenkürzel (Seitenwechsel, Farbschema) bleibt kein Fokusring am zuletzt angeklickten Knopf zurück.
- Beschreibungstexte bei Profilbild und Schwellwerten entfernt. Auf der Geräteseite von Nodes vom Dozenten gibt es keinen Protokoll-Abschnitt mehr.

*Karte und Listen*
- Karte: Das Suchfeld hat die richtige Breite. Die Geräteliste rechts lässt sich über ein Auswahlmenü filtern (Zustand, Herkunft, Merkliste). Mit Strg- oder Cmd-Klick lassen sich mehrere Nodes markieren, sie zeigen ihre Prozent dauerhaft an, die Karte zoomt so weit heraus, dass alle sichtbar sind, und ein weiterer Klick hebt die Markierung wieder auf.
- Die Merkliste gibt es immer, bei keinem Eintrag mit der Zahl 0.
- Blätterleiste: Die Seitenzahlen (die ersten fünf und die letzte) sind anklickbar. Die obere Leiste scrollt die Seite nicht mehr mit, nur die untere springt zur Liste zurück.
- Einsätze: "Zuletzt erledigt" ist deutlicher abgesetzt, die Leerzeile ist eingerückt. Wer einen Einsatz ohne Zuständigen erledigt oder einen Batteriewechsel einträgt, wird automatisch zuständig. Die Einsatzliste blättert zu je 10.

*Rückgängig und Papierkorb*
- Erledigte Einsätze und Batteriewechsel lassen sich zurücknehmen (`POST /api/maintenance/{id}/undo`). Ein schwebender Knopf "Rückgängig" nimmt außerdem die letzte Aktion zurück (Batteriewechsel, Erledigt, Ausmustern, Merkliste leeren) und bleibt eine halbe Minute stehen.
- Touren: Löschen fragt nach. Gelöschte Touren landen im Papierkorb unter Optionen und lassen sich 30 Tage lang wiederherstellen oder endgültig löschen (Migration 0019). Dort stehen auch die ausgemusterten Geräte. Die Tourenliste reagiert beim Darüberfahren.
- Merkliste und Touren liegen jetzt auf dem Server statt im Browser, je Konto, Touren lassen sich teilen (Migration 0018). Was bisher im Browser stand, wird beim ersten Öffnen einmal übernommen.
- Batteriekatalog: "Ändern" springt zum Formular, "Entfernen" ist leicht rötlich.

*Postfach und Hinweise*
- Automatische Weiterleitung nur noch per E-Mail: Der Zugang zum Mailserver (`SMTP_URL`, `SMTP_FROM`) steht einmal in der Server-`.env`, jede Person trägt im Postfach nur ihre Adresse ein. WhatsApp über CallMeBot ist aus der Oberfläche genommen, weil dort jede Nummer einen eigenen Schlüssel braucht (der Server kann es weiterhin, ältere Regeln mit WhatsApp werden beim Speichern abgeschaltet). Telegram und SMS sind ebenfalls nicht mehr zu sehen. Das Skript `deploy/set-smtp.sh` trägt den Zugang auf dem Server ein (Passwort unsichtbar abgefragt, Sonderzeichen kodiert, nur der Operations-Dienst wird neu gestartet), die Anleitung steht in `docs/deployment.md`.
- Gestaltete Alarm-Mail (`services/operations-service/src/mail.rs`): kurz und auf einen Blick. Dunkler Kopf mit dem Logo, farbiges Schild je Art der Meldung (Kritisch, Offline, Ausfall, Hinweis), eine Überschrift, Node und Name, bei Ladestand die große Zahl mit Balken, bis zu drei Kacheln (Spannung, Leer in, Zuletzt), ein Satz zum Handeln und die Knöpfe "Node öffnen" und "Route". Das Logo steckt als eingebettetes Bild in der Mail (keine Bilder aus dem Netz, damit es auch ohne "Bilder laden" erscheint), der Absender heißt "Battery Bandits", der Betreff nennt Art, Node und Ladestand. Jede Mail hat eine Textfassung für Programme ohne HTML und passt sich dem dunklen Modus an. Die Probenachricht nutzt dieselbe Gestaltung in Grün. Vorschau ohne Mailserver: `MAIL_PREVIEW_DIR=dev/mail-preview cargo test -p operations-service --lib write_previews -- --ignored` schreibt HTML und Text je Art nach `dev/mail-preview/`.
- Der Tipp auf "Weiterleiten" an einem Eintrag bietet weiterhin WhatsApp und E-Mail an. Das öffnet die App mit vorbereitetem Text und braucht keine Einrichtung. "Weiterleiten" und "Meine Nodes" öffnen ein Fenster über der Seite.
- Eintragen bei einem Node: Die wichtigsten Angaben der Erklärung sind fett.
- Effizienz: Der Untertitel der Seite ist entfernt. In "Datenhaltung" sind die wichtigen Angaben fett (Größe, Platz, Zuwachs pro Tag, Prognose).

*Anmeldeseite*
- Am PC (ab 768 Pixel Breite) sind Logo und Name auf der linken Fläche größer und wachsen mit dem Fenster (Logo 64 bis 112 Pixel, Name 26 bis 48 Pixel, vorher 44 und 22). Am Handy bleibt die Ansicht unverändert.

*Handy*
- Die Punkte unter den Karten zeigen höchstens 15 auf einmal, darunter steht "Karte X von N". Sie stehen in festen Seiten: Die Punkte bleiben stehen, der aktive wandert nach rechts, und erst nach dem letzten Punkt kommen die nächsten (die letzte Seite ist kürzer). Vorher war das Fenster auf die aktuelle Karte zentriert, der aktive Punkt stand immer in der Mitte und es wirkte, als würden sich die Punkte wiederholen.
- Das Kartenblatt lässt sich am Griff nach unten ziehen und nach oben aufziehen.
- Wartung und Protokoll: Die Reiter scrollen seitlich, die Blätterleiste bricht um, die Seite lässt sich nicht mehr zur Seite schieben.

**Nachbesserungen (14.11.2026)**
- Karte: Dritter Kartenstil "Schwarz-Weiß" (ruhige Grau- und Weißtöne ohne Orange, die bunten Markierungen stechen heraus), dazu "Karte" und "Satellit" als Auswahl. Die Wahl bleibt im Browser gespeichert.
- Karte: Die Suche zeigt unter dem Feld die Treffer, ein Tipp (oder Enter) springt zum Node und blendet den Suchfilter wieder aus.
- Karte: "Nur ausgewählte" (im Filter und im Auswahl-Modus) zeigt nur die Nodes, die man zum Wechseln gemerkt hat.
- Karte: Öffnet man die Details und geht zurück, ist die Geräteliste wieder so offen oder zu wie vorher.
- Karte: Die Geräteliste gleitet ruhig und ohne Überschwingen herein und hinaus (am PC von rechts, am Handy von unten).
- Navigation: Die helle Umrandung gehört dem aktiven Eintrag selbst und liegt genau um Symbol, Name und Zähler (vorher ein gleitendes Kissen mit fester Breite, das bei Zählern und langen Namen daneben lag).
- Konten haben eine eigene Seite (Optionen, "Konten verwalten") mit Suche, Anzeige in Achterschritten und Formular zum Anlegen, jede Zeile klappt erst auf Tipp ihre Knöpfe auf.
- Der Schalter "Hoher Kontrast" ist entfernt.
- Karte am PC: Die Geräteliste ist eine Leiste am rechten Rand, die sich mit dem Griff nach rechts wegschieben lässt (am Handy bleibt es das Blatt von unten). Die Zeilen werden bei wenig Platz nicht mehr zerquetscht.
- Standort-Auswahlkarte: eigene Markierung statt des fehlenden Leaflet-Bildes.
- Tabelle der Geräte: Bei wenig Platz verschwinden die Spalten früher, der Knopf für den Batteriewechsel schrumpft zum Blitz (Name im Tooltip) und bleibt am Zeilenende sichtbar.
- Der Knopf "Weiter" unter den Listen war kaputt (der Code erschien als Text) und funktioniert wieder.
- Dringlichkeits-Leiste: Pfeile links und rechts zum Weiterblättern, neue Karte alle 10 Sekunden, langsames, weiches Gleiten.
- Karte: Umschalter "Satellit" und "Karte" (Straßenkarte hell oder dunkel passend zum Farbschema, ohne Höhendreiecke; Kacheln von Esri, Quelle steht unten rechts auf der Karte). Die Wahl bleibt im Browser gespeichert.
- Eintragen bei einem Node: Erklärung, was das bedeutet, und eine kleine Rückfrage mit "Ja, eintragen". Austragen geschieht sofort.
- Profilbilder: 164 echte Waschbär-Fotos (Wikimedia Commons, freie Lizenzen, Bildnachweis in den Optionen verlinkt). In den Optionen lässt sich das eigene Bild durchblättern, per Zufall würfeln und mit "Übernehmen" speichern (`PUT /api/auth/avatar`).
- Geräte ausmustern: Am Gerät gibt es "Ausmustern" (Grund: alles erledigt, ersetzt, weggeworfen, dazu eine Notiz). Das Gerät verschwindet aus Übersicht, Karte, Zählern und Meldungen, offene Meldungen werden gelöst, geplante Einsätze entfallen. Messwerte und erledigte Einsätze bleiben. Die Überwachung bewertet es nicht mehr. Unten in der Übersicht steht die Liste der ausgemusterten Geräte mit "Wieder aufnehmen" (Migration 0016, `POST` und `DELETE /api/devices/{id}/retire`, `GET /api/devices?retired=only`).
- Batteriewechsel: Der Ladestand der neuen Batterie lässt sich als Zahl eintippen (0 bis 100 Prozent), wer ihn nicht kennt, lässt es auf "Automatisch".
- Position am Gerät: oben der Knopf "Auf der Karte zeigen" (öffnet die Karte und zoomt auf das Gerät), darunter klein die Koordinaten.
- Prognose-Satz: Ist die Batterie schon leer oder unter dem kritischen Wert, steht das da statt "in weniger als 1 Std.".
- Effizienz: Alle Stufen heißen "Schätzung" und ein Hinweis nennt die Grundlage (nur Messwerte der TrafficNodes und die Nennkapazität). Von Hand eingetragen wird nur der Preis einer Batterie, auch nachträglich ("Preis eintragen" an der laufenden und an früheren Batterien, `PUT /api/devices/{id}/battery-price`, `PUT /api/battery-cycles/{id}/price`). Der Preis fließt nur in die Kosten ein.
- Effizienz-Seite: "Geräte im Vergleich" mit Suche, Stufenfilter und Auswahl der Kategorie (Effizienz, Haltbarkeit, Preis pro Tag, Leistung als Innenwiderstand, Spannungsabfall, Stromverbrauch), Reihenfolge umkehrbar. Es wird immer nur eine Seite mit zehn Geräten geladen, die Blätterleiste steht oben und unten. Die Geräteliste der Übersicht nutzt dieselbe Leiste.
- Verworfene Nachrichten: Unter "Datenhaltung" öffnet "mit Grund protokolliert" die Liste (Grund, Zeit, Thema, Inhalt, seitenweise, neueste zuerst, `GET /api/invalid-messages`).
- Postfach: höchstens zehn Einträge je Seite (Blätterleiste oben und unten), Suche und Umschalter "Meine Nodes" (Standard, Meldungen der eingetragenen Nodes plus alle Zuweisungen und Kommentare) oder "Alle Meldungen". Gelesenes bleibt an seinem Platz, nur der Punkt verschwindet (vorher sprang es nach unten).
- Batterie gewechselt: statt der langen Auswahlliste ein Suchfeld, in das man die Nummer oder den Namen tippt; es zeigt die passenden Nodes, das dringendste ist vorgewählt.
- Einsätze: Unten in jeder Zeile steht "Zuständig: Name" (oder "Zuständig: niemand").
- Geräteseite: Der Pfeil oben führt dorthin zurück, woher man kam (Wartung, Postfach, Karte ...), nicht immer zur Übersicht.
- Karte: Der Knopf mit dem Lesezeichen schaltet die Auswahl ein. Dann wählt ein Tipp auf einen Marker den Node zum Wechseln aus (er leuchtet auf und kommt auf die Merkliste), mit Zähler, "Route dorthin" und "Auswahl leeren".
- Beispieldaten: Auf erfundenen Geräten (Staging, lokal) legt der Einsatz-Dienst einmalig drei Beispiel-Einsätze mit Kommentaren an, die auch im Postfach auftauchen. Echte Geräte bleiben unberührt.
- Handy: "Zu tun" und die Merkliste lassen sich auf- und zuklappen (am Handy zunächst zu, der Zähler bleibt sichtbar). In der Merkliste ragen Knopf, Fehlertext und Zeilen nicht mehr über den Rand. Das X im Geräteblatt läuft beim Scrollen mit.
- Karte: Der Knopf zum Zentrieren heißt jetzt "Mein Standort" (Symbol mit Text, Tooltip erklärt es). Am Handy steht die Suche in einer eigenen Zeile über den Knöpfen.
- Batteriekatalog (Wartung, "Batterien"): Wer Batterien bestellt, legt sie mit Name, Preis und Notiz an, ändert den Preis oder nimmt sie heraus (Migration 0017, `GET/POST /api/battery-types`, `PUT/DELETE /api/battery-types/{id}`). Beim Batteriewechsel wählt man die Batterie in einer Liste, Name und Preis kommen vom Server aus dem Katalog; "Andere Batterie" erlaubt weiter freie Eingabe. Entfernte Batterien bleiben archiviert, frühere Wechsel behalten ihre Angaben.
- Tour: Oben zwei Knöpfe. "Meine Einsätze" zeigt nur die eigenen Einsätze. "Tour planen" legt benannte Touren an, in die man die Nodes einträgt (Suche nach Nummer oder Name, oder die Merkliste übernehmen). Daraus entstehen die schnellste Reihenfolge und der Link, der die Tour in Google Maps startet. Die Touren liegen im Browser dieses Geräts.
- Protokoll: höchstens zehn Einträge je Seite mit Blätterleiste oben und unten, auch am Gerät. Die Summe der Kilometer zeigt bei einem einzigen Halt nicht mehr "-0,0".
- Effizienz, "Datenhaltung": Zeigt die Größe der Datenbank und wie viel vom vorgesehenen Platz belegt ist (Balken, ab 80 Prozent rot), wie schnell die Messwerte wachsen und wie groß die Datenbank bei zwölf Monaten Aufbewahrung wird, samt Hinweis, ob das in den Platz passt. Der vorgesehene Platz steht in `DB_QUOTA_MB` (Vorgabe 5 GB), die Datenbank kennt ihre Festplatte nicht.
- Konten: Das feste Konto `admin` behält für immer alle Rechte (nicht herabstufbar, nicht löschbar, auch wenn die Konfiguration es nicht als Admin kennzeichnet). Die Kontenliste ist am PC eine Liste mit Spalten (Name, Benutzername, Rolle, Seit). Mitarbeiter sehen die Kontenverwaltung nicht und die Namensliste zum Zuweisen verrät nicht, wer Admin ist (durch Tests belegt).
- Standort: Gibt der Browser den Standort nicht frei, gilt der eingetragene Standardstandort ohne Fehlermeldung (Route, Tour, Karte). Der Bildnachweis-Hinweis unten im Profil ist entfernt, die Datei `/avatars/BILDNACHWEIS.txt` bleibt.
- Lokale Test-Konten: `noah`, `fynn` und `admin` (Passwörter siehe `deploy/.env.local.example`, nur für den lokalen Test).
- Staging zeigt neben den echten Daten erfundene Testgeräte (Profil `sim`, siehe `docs/deployment.md`). Bei erfundenen Geräten gleicht die Erfassung den Ladezustand mit der Spannung ab, wenn der Simulator neu startet und die Integration überholt ist.

**Übersicht und Navigation**
- Der große Kopfbereich ist durch eine Dringlichkeits-Anzeige ersetzt: kritische, ausgefallene und auffällige Geräte laufen als Karten durch (von selbst oder per Wischen). Jede Karte zeigt, wie lange das Gerät schon in diesem Zustand ist, zum Beispiel "Offline seit 2 Std. 14 Min.". Der Zeitpunkt des Statuswechsels steht in der Datenbank (`status_changed_at`).
- Schneller Seitenwechsel: Besuchte Seiten bleiben im Speicher, Daten werden nur bei Änderung neu gezeichnet, die Anfragen laufen gleichzeitig, die Karte baut sich im nächsten Bild auf, die Unschärfe über scrollendem Inhalt ist entfernt.
- Zwischen den sechs Seiten (Übersicht, Karte, Wartung, Postfach, Effizienz, Optionen) wechselt man auch durch Wischen. Auf der Karte, im Diagramm und in Reglern ist das Wischen abgeschaltet.
- Fehlermeldungen liegen über dem Inhalt und lassen sich nach links, rechts oder oben wegwischen. Solange das Problem besteht, zeigt die Kopfzeile statt "Live" ein rotes Warndreieck, ein Tipp zeigt die Einzelheiten.
- Die Karte bleibt frei: schmale Suchleiste oben, einklappbares Blatt unten, keine Unschärfe. Tippflächen sind mindestens 48 Pixel groß.

**Wartung**
- Batteriewechsel mit zwei Tipps: Knopf auf der Gerätekarte, in der Tabelle, in der Merkliste, auf der Geräteseite und im Reiter Wartung, dann "Speichern". Gerät und Zeitpunkt sind vorbelegt. Den Ladestand muss niemand kennen: Die Batterien kommen geladen zurück, das System schätzt den Stand mit der ersten Messung aus der Spannung (wer ihn weiß, wählt 100, 90 oder 80 Prozent). Techniker (in Kurzform "Noah B.", bei vielen Personen mit Suche), Preis in Euro und Notiz sind optional. Ein geplanter Einsatz wird abgeschlossen, sonst entsteht ein erledigter. Der alte Zyklus wird mit seinem Preis für den Vergleich festgehalten, die Kosten je Tag stehen in der Effizienz.
- Geräte anpinnen ("Merken") in Übersicht, Karte und Gerät. Die Merkliste berechnet vom eigenen Standort (Browser, sonst Standardstandort aus den Optionen) die kürzeste Reihenfolge (bis acht Halte exakt) und öffnet sie als mehrstufige Route in Google Maps.

**Postfach**
- Jeder Eintrag lässt sich mit einem Tipp über WhatsApp, Telegram, SMS oder E-Mail weiterleiten (vorbereiteter Text mit Link zum Gerät).
- Regel je Person für die automatische Weiterleitung neuer Meldungen, optional nur kritische. Der Server sendet sie, sobald die Zugänge in seiner Umgebung stehen (`TELEGRAM_BOT_TOKEN`, `WHATSAPP_WEBHOOK_URL`, `SMS_WEBHOOK_URL`, `SMTP_URL`, `SMTP_FROM`, `PUBLIC_URL`). Probenachricht und Protokoll inklusive.

**Effizienz**
- Ehrlich statt schön: Eine Effizienz gibt es nur mit belastbarer Grundlage (mindestens 50 Messwerte, 24 Stunden, Last und ein Spannungsabfall von mindestens 150 mV). Reicht sie nicht oder ergäbe sich mehr als 120 Prozent der Nennkapazität, steht "keine Aussage" mit Begründung da, die gemessenen Werte bleiben sichtbar.
- Neue Seite und neue Sektion im Gerät: mittlerer Strom, Spannungsabfall pro Tag, reale gegen rechnerische Laufzeit bei 80 Ah, Effizienz in Prozent, Spannung nach Temperaturbereich, Spannung unter hoher Last und Vergleich mit früheren Batteriezyklen. Dazu der Datenbestand (Zahl der Messwerte, Zeitraum, verworfene Nachrichten).

**Postfach als Eilmeldungs-Postfach**
- Das Postfach zeigt oben die Meldungen. Mit **Weiterleiten** öffnet sich der Bereich für WhatsApp, Telegram, SMS und E-Mail samt Test-Knopf, mit **Meine Nodes** trägt man sich bei Nodes ein (mit Suche).
- Wer sich bei einem Node einträgt, bekommt dessen Eilmeldungen im Postfach und auf dem Handy. Hat ein Node niemanden eingetragen, bekommen alle die Meldung, zum Beispiel wenn die Batterie leer ist. Eintragen geht auch am Gerät.
- Lokal fängt Mailpit (http://localhost:8025) alle E-Mails ab, damit Weiterleitung und Test-Knopf ohne Einrichtung funktionieren. Auf dem Server braucht jeder Dienst seinen Zugang (siehe docs/deployment.md).

**Effizienz nach Modell**
- Beim Batteriewechsel lässt sich das Modell eintragen (das letzte ist vorbelegt). Die Effizienz fasst gleiche Modelle zusammen: Laufzeit, Verbrauch in Ah pro Tag, Preis, Kosten je Tag und je Ah, Innenwiderstand und Effizienz, das günstigste Modell je Tag vorn.
- Spannungsabfall wie im Alltag gerechnet: Der Innenwiderstand folgt aus der Steigung von Spannung über Strom (`U = I * R`), mit Einordnung (gut bis 15 mΩ) und dem Abfall beim mittleren Strom und beim Senden mit 5 A. Ohne schwankenden Strom und mindestens 100 Messwerte gibt es keine Aussage.

**Listen und Bedienung**
- Lange Listen zeigen zuerst 20 Einträge (Merkliste 10) und laden mit "Weitere anzeigen" nach: Einsätze, Postfach, Protokoll, Nodes. Die Geräteliste blättert zu je 20 Geräten. Ein anderer Filter lässt die Seite nicht springen.
- Die Karte springt zum eigenen Standort und zeigt ihn mit einem eigenen blauen Punkt, dazu ein Knopf "Zu meinem Standort".
- Standardstandort wählt man auf einer Karte (ein Tipp), über einen Link aus Google Maps oder mit dem Standort des Telefons, nicht mehr mit Koordinaten. Die Darstellung ist eine schmale Leiste am Ende der Optionen, das Farbschema folgt standardmäßig dem System.

**Konten, Profil und Anmeldung**
- Die Anmeldeseite ist schlicht: Logo, Benutzername, Passwort und "Angemeldet bleiben". Der Hinweis zur Sperre erscheint nur, wenn man fünfmal falsch eingegeben hat.
- Angemeldet bleiben: Das Token liegt als HttpOnly-Cookie im Browser und gilt gut ein Jahr (sonst 14 Tage). Das Passwort lässt sich im Profil ändern, andere Geräte werden dann abgemeldet. Ein selbst gewähltes Passwort überschreibt die Konfiguration (BB_USERS) beim Start nicht mehr.
- Profil: Oben rechts steht das Profilbild statt des Namens. Ein Tipp öffnet das Profil, von dort gehen das Zahnrad zu den Optionen und das Abmelden. Die Navigation unten hat nur noch fünf Seiten, die Optionen liegen hinter dem Profil. Das Farbschema (Vorgabe: wie das System) wechselt man nur noch in den Optionen.
- Profilbilder: 100 echte Waschbär-Fotos von Wikimedia Commons (freie Lizenzen, Nachweis in `frontend/avatars/BILDNACHWEIS.txt`, im Profil verlinkt), jedes Konto bekommt bei der Anlage zufällig eines.
- Admin-Rolle: Registrieren kann sich niemand selbst. Admins legen in den Optionen Konten an, vergeben die Rolle, setzen Passwörter zurück und löschen Konten (nicht das eigene, nicht den letzten Admin). In BB_USERS macht ein viertes Feld `admin` zum Admin (`noah:passwort:Noah Boufercha:admin`). Gibt es keinen Admin, wird das erste Konto der Liste einer.
- WhatsApp ohne Anbieter-Vertrag: Jede Person schaltet einmalig ihren persönlichen CallMeBot frei (kostenlos) und trägt den Schlüssel im Postfach ein. Der Server schickt dann Eilmeldungen über callmebot.com an die eigene Nummer. Das ist keine perfekte, aber eine funktionierende Lösung. Mit `WHATSAPP_CALLMEBOT=off` abschaltbar.

**Weiteres**
- Mehr erfundene Geräte zum Ausprobieren: Der Simulator sendet mit `SIM_DEVICES` beliebig viele (lokal 60, höchstens 500), erkennbar an der Kennung SIM01 bis SIMnn und der Kennzeichnung "Erfunden" in Tabelle, Anzeige, Merkliste, Effizienz und Karte (blasser Punkt).
- Teilen am Gerät: Die Statistik als aufgereihter Text (jede Angabe eine Zeile) per Teilen-Fenster des Handys, WhatsApp, Telegram, SMS, E-Mail oder Kopieren. Die Messwerte als CSV bleiben als kleiner Link. "Eigene Seite" heißt jetzt "Große Ansicht".
- Protokoll: Kommentare lassen sich ausklappen und zeigen den Wortlaut.
- Die Kopfleiste liegt über allem (außer Dialogen), das Geräteblatt beginnt darunter, die Gerätekarte klappt ein, sobald ein Gerät geöffnet wird. Der Punkt über der Anzeige leuchtet nur grün, wenn nichts zu tun ist.

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
