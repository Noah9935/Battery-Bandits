# Änderungsprotokoll

## 0.2.0, für das Handy gebaut (November 2026, in Arbeit)

Die Oberfläche ist jetzt zuerst für das Handy im Einhandbetrieb gedacht.

**Nachbesserungen (15.11.2026)**

*Simulator und Daten*
- Der Simulator ist jetzt ein MQTT-Broker mit 50 festen Nodes (SIM01 bis SIM50) rund um Stuttgart, Leonberg und die Filder. Sie senden exakt wie die Nodes der Vorlesung (Topic `traffic-nodes/<Kennung>/measurements`, dasselbe JSON, Strom im Entladebetrieb negativ) und unterscheiden sich nur durch die Kennung SIMnn und die Herkunft `simulation` ("Erfunden" in der Oberfläche). Ort, Szenario und Startladung jedes Nodes stehen fest in einer Tabelle, es gibt keine Zufallsgeräte mehr (vorher 12 feste und beliebig viele zufällige bis 500). Fünf Nodes starten kritisch, rund sechs mit Warnung, zwei haben nie GPS, vier verstummen nach einigen Runden. Die Entladung läuft auf Staging in Echtzeit (`SIM_SPEEDUP=1`), lokal bleibt sie beschleunigt. Auf die Nodes der Vorlesung wechselt man, indem man das Profil `sim` weglässt, siehe `docs/deployment.md`.
- Die erfundenen Beispiel-Einsätze mit Kommentaren sind weg (`seed_demo_tasks` entfernt). Migration 0021 räumt einmalig alles auf, was für erfundene Geräte vorher entstanden ist (Geräte bis SIM60 mit Messwerten, Meldungen, Einsätzen, Kommentaren, Merklisten, Touren, Postfach und Protokoll), der Simulator legt SIM01 bis SIM50 mit der nächsten Nachricht neu an. Echte Geräte bleiben unberührt, auf Prod ändert sich nichts.
- Der Batteriekatalog ist vorbelegt (Migration 0020): fünf 12-V-AGM-Batterien der Klasse 80 bis 100 Ah mit Händlerpreisen, Stand November 2026 (Effekta BTL 12-80 für 151,18 Euro, BTL 12-100 für 175,63 Euro, Victron AGM Deep Cycle 90 Ah für 212,62 Euro, Dometic NDS GreenPower GP80 für 255,00 Euro, Electronicx AGM Marine 80 Ah für 115,99 Euro). Kein Hersteller nennt diese Typen ausdrücklich für Ampeln, es sind die gängigen der Klasse. Der Operations-Dienst baut sie den erfundenen Geräten fest ein (SIM01 das erste Modell, SIM02 das zweite, SIM06 wieder das erste), damit Preis pro Tag und der Modellvergleich in der Effizienz echte Zahlen haben.
- Der Platz der Datenbank in "Datenhaltung" ist der echte: die Datenbank plus der freie Platz auf dem Server (`statvfs`). `DB_QUOTA_MB` ist nur noch ein optionaler fester Wert, leer heißt echter Platz.

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
- Die Punkte unter den Karten zeigen höchstens 15 auf einmal, das Fenster wandert mit, darunter steht "Karte X von N".
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
