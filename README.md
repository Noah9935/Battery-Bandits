# Battery Bandits – TrafficNode Batterieüberwachung

> **Hinweis:** Dieses Projekt ist Teil der Lehrveranstaltung *Software Engineering I*, DHBW Stuttgart (TINF25D-KI)
> **Spezifikation v0.1**

<p align="center">
  <img src="docs/images/ReadMePic.jpg" alt="Waschbär" width="500">
</p>

---

## 1. Kontext

### a. Beschreibung

Ein Unternehmen betreibt mobile, batteriebetriebene Geräte (**TrafficNodes**) ohne digitale Zustandsüberwachung. Schwache oder leere Batterien führen zu Geräteausfällen und ungeplanten Serviceeinsätzen.

**Battery Bandits** empfängt die Batteriedaten der TrafficNodes über einen MQTT-Broker, speichert sie und stellt sie in einem Dashboard dar. Das bringt zwei Vorteile:

- Der Batteriezustand lässt sich aus der Ferne beobachten und sein Verlauf nachvollziehen.
- Batteriewechsel werden rechtzeitig geplant, unnötige Wechsel vermieden.


### b. Stakeholder

| Stakeholder | Beschreibung | Ziel / Interesse |
|--------------|--------------|------------------|
| **Auftraggeber** | Fachliche Instanz (Dozent), nimmt das Ergebnis ab | Nachvollziehbare, prüfbare Lösung |
| **Servicetechniker** | Hauptnutzende, wechseln Batterien vor Ort | Wissen, welche Batterie wann fällig ist |
| **Betrieb / IT** | Betreibt die Software | Stabiler, wartbarer Betrieb |
| **Hardware-/Firmware-Team** | Nachbarsystem, liefert die Messwerte | Klar definiertes Nachrichtenformat |
| **Entwicklungsteam** | Studentisches Projektteam | Klare, prüfbare Anforderungen |
---

## 2. Funktionale Anforderungen

Priorität:  Hoch (Muss) ·  Mittel (Soll) ·  Niedrig (Kann)

| ID | Anforderung | Beschreibung | Quelle | Priorität | Prüfkriterium |
|----|-------------|--------------|--------|-----------|---------------|
| F01 | **Datenempfang** | Das System empfängt Batteriemesswerte der TrafficNodes über den MQTT-Broker. | Auftraggeber |  Hoch | Eine auf das Topic publizierte Testnachricht ist nach ≤ 5 s gespeichert. |
| F02 | **Datenspeicherung** | Jeder Messwert wird mit Geräte-ID, Zeitstempel und Ladestand gespeichert. | Auftraggeber |  Hoch | Jede empfangene Nachricht ergibt genau einen Datensatz mit allen drei Feldern. |
| F03 | **Geräteübersicht** | Nutzende sehen den aktuellen Batteriezustand aller Geräte. | Servicetechniker |  Hoch | Jedes Gerät erscheint mit letztem Wert und Zeitpunkt; Abgleich mit der DB stimmt. |
| F04 | **Verlaufsansicht** | Nutzende sehen den Ladestand eines Geräts über einen wählbaren Zeitraum. | Servicetechniker |  Hoch | Bekannte Testdaten der letzten 7 Tage werden korrekt dargestellt. |
| F05 | **Kritisch-Markierung** | Geräte unter einem konfigurierbaren Schwellwert werden markiert. | Servicetechniker |  Hoch | Testwert unter dem Schwellwert werden als „kritisch“ markiert. |
| F06 | **Validierung** | Ungültige Nachrichten werden verworfen und protokolliert. | Entwicklungsteam |  Hoch | Fehlerhafte Nachricht erzeugt einen Log-Eintrag und keinen Datensatz. |
| F07 | **Offline-Erkennung** | Geräte ohne Daten seit einem konfigurierbaren Intervall werden erkannt. | Betrieb |  Mittel | Gerät ohne Nachricht seit bestimmten Intervall wird als „offline“ angezeigt. |
| F08 | **Restlaufzeit-Prognose** | Das System schätzt die verbleibende Laufzeit anhand des Verlaufs. | Einsatzplanung |  Mittel | Bei linearem Testdatensatz weicht die Prognose um ≤ 10 % ab. |
| F09 | **Benachrichtigung** | Nutzende werden bei kritischem Zustand aktiv informiert (z. B. E-Mail). | Servicetechniker |  Niedrig | Kritischer Testwert erzeugt nach ≤ 1 min eine Benachrichtigung. |
| F10 | **Login** | Zugriff auf das Dashboard nur für angemeldete Nutzende. | Betrieb |  Niedrig | Aufruf ohne Login leitet auf die Anmeldeseite um. |

---

## 3. Nicht-funktionale Anforderungen

| ID | Kategorie | Beschreibung | Quelle | Priorität | Prüfkriterium |
|----|-----------|--------------|--------|-----------|---------------|
| NF01 | **Performance** | Neue Messwerte sind zeitnah sichtbar. | Servicetechniker |  Hoch | Neuer Wert erscheint im Dashboard nach ≤ 10 s. |
| NF02 | **Skalierbarkeit** | Das System verarbeitet die Last aus dem Mengengerüst. | Mengengerüst |  Hoch | Lasttest mit simulierten Geräten laut Abschnitt 7 ohne Nachrichtenverlust. |
| NF03 | **Zuverlässigkeit** | Verbindungsabbrüche zum Broker werden automatisch behoben. | Betrieb |  Mittel | Nach Broker-Neustart ist die Verbindung nach ≤ 30 s wiederhergestellt. |
| NF04 | **Usability** | Kritische Geräte sind ohne Suche erkennbar. | Servicetechniker |  Mittel | Testperson findet alle kritischen Geräte in < 30 s ohne Einweisung. |
| NF05 | **Testbarkeit** | Kernlogik ist automatisiert getestet. | Entwicklungsteam |  Hoch | Testabdeckung der Kernlogik ≥ 70 %, CI-Pipeline grün. |
| NF06 | **Portabilität** | Das System ist per Container startbar. | Betrieb |  Mittel | `docker compose up` startet alle Komponenten auf einem frischen Rechner. |
| NF07 | **Wartbarkeit** | Schwellwerte und Intervalle sind ohne Codeänderung anpassbar. | Betrieb |  Mittel | Änderung in der Konfiguration wirkt nach Neustart ohne neuen Build. |

---

## 4. Abgrenzung & MVP

Das Projekt wird **inkrementell** entwickelt. Ziel ist zunächst ein **MVP**, das den Kernnutzen demonstriert. Funktionen außerhalb des MVP werden als **Future Work** dokumentiert.

### MVP (Umfang des Projekts)
- Datenempfang und -speicherung ab dem MQTT-Broker (F01, F02, F06)
- Geräteübersicht mit Kritisch-Markierung (F03, F05)
- Verlaufsansicht pro Gerät (F04)
- Simulierte TrafficNodes als Testdatenquelle

### Nicht Teil des MVP
- Offline-Erkennung, Prognose, Benachrichtigung, Login (F07–F10 → Future Work)
- Tourenplanung oder Auftragsverwaltung für Techniker
- Native Mobile-App

### Außerhalb des Projekts
- Hardware, Sensorik und Firmware der TrafficNodes
- Betrieb und Konfiguration des MQTT-Brokers
- Überwachung anderer Gerätewerte als der Batterie

---

## 5. Dictionary / Gemeinsames Begriffsverzeichnis

Das Dictionary legt fest, was wir im Projekt unter einem Begriff verstehen. Die englischen Entsprechungen gelten für Bezeichnungen im Code, in Topics und APIs. Ein Glossareintrag erweitert nicht den MVP-Umfang.

| Bevorzugter Begriff | Englisch / Code | Definition im Projekt | Abgrenzung / alternative Bezeichnungen |
|--------------------|-----------------|-----------------------|---------------------------------------|
| **TrafficNode** | Node | Mobiles, batteriebetriebenes Gerät des Auftraggebers, das Messwerte sendet. | „Gerät“ wird synonym verwendet. Hardware und Firmware liegen außerhalb des Projekts. |
| **Messwert** | Reading | Einzelne Meldung eines TrafficNodes mit Geräte-ID, Zeitstempel und Ladestand. | Nicht mit „Nachricht“ gleichsetzen: eine ungültige Nachricht ergibt keinen Messwert. |
| **Nachricht** | Message | Rohdaten, die über MQTT empfangen werden. | Wird erst nach erfolgreicher Validierung zum Messwert. |
| **Ladestand** | State of Charge (SoC) | Verbleibende Batteriekapazität in Prozent. | Ob Spannung oder Prozent übertragen wird, ist offen (Frage 2). |
| **MQTT-Broker** | Broker | Server, der MQTT-Nachrichten zwischen Sendern und Empfängern verteilt. | Systemgrenze: Wir nutzen den Broker, betreiben ihn aber nicht. |
| **Topic** | Topic | Adresse, unter der MQTT-Nachrichten veröffentlicht werden. | Struktur noch mit dem Auftraggeber zu klären (Frage 1). |
| **Schwellwert** | Threshold | Konfigurierbarer Ladestand, unter dem ein Gerät als kritisch gilt. | Wert und ggf. Gerätespezifik noch offen (Frage 4). |
| **Kritisch** | Critical | Zustand eines Geräts, dessen letzter Ladestand unter dem Schwellwert liegt. | Nicht gleich „offline“. |
| **Offline** | Offline | Gerät hat länger als das definierte Intervall keinen Messwert gesendet. | Sagt nichts über den Ladestand aus. |
| **Verlauf** | History | Zeitliche Abfolge der Messwerte eines Geräts. | — |
| **Batteriewechsel** | Battery replacement | Serviceeinsatz zum Austausch der Batterie vor Ort. | Wird vom System nicht durchgeführt, nur vorbereitet. |

### Pflege und offene Begriffsentscheidungen

- **Ein Begriff pro Bedeutung:** Synonyme zuordnen, unterschiedliche Konzepte getrennt benennen.
- **Unklarheiten früh erfassen:** Neue Begriffe vor der Umsetzung ergänzen und offene Definitionen markieren.
- **Änderungen gemeinsam prüfen:** Begriffsänderungen per Pull Request; betroffene Anforderungen, Tests und Code-Bezeichnungen mitprüfen.
- **Offene Punkte:** Einheit des Ladestands (%/V), Höhe des Schwellwerts, Länge des Offline-Intervalls.

---

## 6. Randbedingungen

- Datenquelle ist ausschließlich der MQTT-Broker (vorgegeben)
- Projektlaufzeit: ein Semester, Team aus drei Personen
- Versionsverwaltung und Projektmanagement über GitHub
- Dokumentation in Markdown im Repository

---

## 7. Mengengerüst

| Größe | Annahme |
|-------|---------|
| Anzahl TrafficNodes | ca. 500 |
| Sendeintervall pro Gerät | 1 Nachricht / 5 min |
| Nachrichten pro Tag | ca. 144.000 |
| Größe pro Nachricht | < 1 KB |
| Aufbewahrung der Messwerte | 12 Monate |
| Gleichzeitige Nutzende im Dashboard | ≤ 20 |

---

## Team & Organisation

| Rolle | Name | Matrikelnummer |
|-------|------|----------------|
| _<Rolle>_ | Noah Boufercha | 1455982 |
| _<Rolle>_ | Fynn Becker | 3487242 |

---

## Tools & Technologien
- **Messaging:** MQTT
- **Backend:** _
- **Frontend:** _
- **Datenbank:** _
- **Deployment:** Docker Compose
- **Projektmanagement:** GitHub Projects

---

## 8. Offene Fragen an den Auftraggeber

1. Welches Nachrichtenformat und welche Topic-Struktur senden die TrafficNodes?
2. Wird Spannung, Ladestand in % oder beides übertragen?
3. Wie viele Geräte gibt es, und wie oft senden sie? (→ Mengengerüst)
4. Ab welchem Wert gilt eine Batterie als kritisch? Ist der Wert gerätespezifisch?
5. Wie lange müssen Messwerte aufbewahrt werden?
6. Wer nutzt das System, und brauchen Nutzende unterschiedliche Rechte?
7. Sollen Benachrichtigungen aktiv versendet werden, und über welchen Kanal?
8. Gibt es Vorgaben zu Technologie-Stack oder Betriebsumgebung?
