# Ladezustand und Restlaufzeit mit Fehlerangabe

Dieses Dokument beschreibt, wie Battery Bandits den Ladezustand (SoC) und die Restlaufzeit schätzt, warum zu jeder Zahl
eine Fehlerangabe gehört und wie die Angaben geprüft wurden. Der Code liegt in `crates/shared/src/estimator.rs`
(Schätzer), `crates/shared/src/forecast.rs` (Prognose), `services/ingestion-service/src/soc_state.rs` (Anbindung an die
Datenbank) und `services/monitoring-service/src/` (Status und Prognose).

## Ziel

Ein Ladezustand von "72 %" ist ohne Fehlerangabe wenig wert. Gemessen wird nur Spannung, Strom und Temperatur, der
Ladezustand ist immer geschätzt. Wer eine Batterie wechseln muss, braucht keine Scheingenauigkeit, sondern eine Zahl,
auf die er sich verlassen kann, und die Angabe, wie weit sie danebenliegen kann. Deshalb gibt das System überall den
Wert und seine Unsicherheit an ("72 %, ± 6 Punkte") und rechnet Alarme mit dem vorsichtigen Wert.

## Verfahren

Zwei Quellen sagen etwas über den Ladezustand, beide sind für sich unzuverlässig:

| Quelle | Stärke | Schwäche |
| --- | --- | --- |
| Ladungsintegration (Vorlesung: `SOC = SOC(Start) − 100 · Q / 80 Ah`, Trapezregel) | kurzfristig sehr genau | driftet: Offset des Stromsensors, Verstärkungsfehler und abweichende Kapazität summieren sich, bei Datenlücken fehlt der Strom |
| Ruhespannung über die Spannungskurve der Batterie | absolut, ohne Drift | verrauscht, hängt von Last und Temperatur ab, die Kurve selbst ist nur auf einige Prozent genau |

Ein erweitertes Kalman-Filter führt beide zusammen (so machen es Batteriemanagement-Systeme). Es hat drei Zustandsgrößen:

1. den Ladezustand `s` in Prozent,
2. einen Offset `b` des Stromsensors in Ampere,
3. einen relativen Faktor `g` für Verstärkungsfehler des Sensors und Abweichung der Kapazität von den 80 Ah.

Die entnommene Ladung ist `(I − b) · (1 + g) · Δt`. Weil `b` und `g` mitgeschätzt werden, lernt das Filter die Drift der
Integration anhand der Spannung selbst. Je länger ein Gerät mit gültigen Messwerten läuft, desto genauer wird es.

**Fortschreibung.** Bei jeder Messung wird mit der Trapezregel über die Zeit seit der letzten Messung integriert. Die
Unsicherheit wächst dabei durch die Unsicherheit von `b` und `g` und eine kleine Selbstentladung.

**Abgleich mit der Spannung.** Aus der Klemmenspannung wird die Ruhespannung (Spannungsabfall am Innenwiderstand
zurückgerechnet, Temperatur korrigiert) und daraus über die Kurve ein Ladezustand. Wie stark das Filter dieser Messung
glaubt, hängt von ihrer Güte ab: Unter Last und an flachen Stellen der Kurve ist sie unsicherer, bei kleinen Strömen und
$1Das Rauschen der Spannungsmessung ist an den echten Reihen gemessen (Streuung aufeinanderfolgender Werte von Gerät 101: rund
3 mV), angesetzt sind 5 mV als Reserve. Temperatur, Innenwiderstand und Kurvenfehler sind dagegen nicht messbar und gehen
weiterhin als Unsicherheit ein, ebenso der feste Anteil von 3 Punkten.

**Datenlücken.** Ist die Pause länger als 30 Minuten, gilt der Strom dazwischen als unbekannt. Es wird dann nichts
integriert, was gemessen wurde, sondern mit dem gleitenden Mittel des Stroms (Zeitkonstante 6 Stunden) gerechnet und die
Unsicherheit stark erhöht (50 Prozent des angenommenen Stroms). Die nächste Spannungsmessung holt den Wert zurück. Die
frühere Rechnung hätte über eine Pause von drei Tagen bei 500 mA etwa 45 Prozentpunkte abgezogen, die nie entnommen
wurden, und das Gerät fälschlich als fast leer gemeldet. Die seit dem Einbau entnommene Ladung (`discharged_ah`) zählt nur
lückenlose Messungen und ist über eine Lücke eine Untergrenze.

**Selbstkontrolle.** Zu jeder Messung wird die Abweichung zwischen Spannung und Erwartung durch die erwartete Streuung
geteilt und quadriert (NIS). Bei passendem Modell liegt der Mittelwert bei 1 oder darunter. Ein gleitender Mittelwert über
rund 50 Messungen (`fit`, im Zustand gespeichert) verbreitert die angegebene Spanne mit `√fit`, sobald er über 1 steigt, und
macht sie nie schmaler. Ein einzelner Ausreißer zählt höchstens 16. So wird eine Batterie, die nicht zur Standardkurve passt
(anderer Typ, defekter Sensor), von selbst mit größerer Spanne angegeben, statt sich sicher zu irren. Steigt `fit` über 3,
steht eine Warnung im Protokoll des Erfassungs-Dienstes. Grenze: Ein reiner Versatz der Kurve ist von einem falschen
Startwert nicht zu unterscheiden und fällt deshalb nicht auf, dafür ist der feste Anteil da.

$1 Weicht eine Spannungsmessung um mehr als vier Standardabweichungen von der Erwartung ab, wird sie nicht
verwendet. Zwölf Abweisungen in Folge zeigen, dass der Zustand selbst falsch ist (zum Beispiel ein nicht gemeldeter
Batteriewechsel): Dann beginnt das Filter neu aus der Spannung.

**Batteriewechsel.** Ein angegebener Ladezustand der neuen Batterie wird mit drei Prozentpunkten Unsicherheit übernommen
und mit der nächsten Spannung geprüft. Widerspricht die Spannung dauerhaft (100 Prozent angegeben, die Spannung zeigt die
Hälfte), setzt sich die Spannung durch. Ohne Angabe beginnt das Filter bei der ersten Messung aus der Spannung, mit 4 Punkten Zuschlag (die erste Messung kann
in einem Übergang liegen, siehe TN0001), höchstens aber mit 30 Punkten Unsicherheit, so viel wie reines Raten. Der
Zustand des Filters liegt je Gerät als JSON in `devices.estimator_state`, er übersteht Neustarts. Ist er unlesbar, beginnt
das Filter neu, es geht kein Messwert verloren.

## Fehlerangabe

Zu jedem Ladezustand gehört eine Standardabweichung `σ` in Prozentpunkten. Sie besteht aus zwei Teilen:

- dem Teil, den das Filter aus seiner Kovarianz kennt (wächst bei Lücken und Last, schrumpft mit guten Messungen),
- einem festen Teil von 3 Prozentpunkten für den Fehler der Spannungskurve. Ein Versatz der Kurve ist von einem falschen
  Startwert nicht zu unterscheiden, mehr Messungen verkleinern ihn deshalb nicht. Die Kurve stammt von einer einzigen
  gemessenen Batterie, andere Typen und alte Batterien weichen um mehrere Punkte ab.

Die Oberfläche zeigt den 95-Prozent-Bereich (1,96 · σ): "72 %, ± 6 Punkte". Im Verlauf liegt er als Fläche um die Kurve.
Gespeichert sind `σ` am Gerät (`devices.soc_sigma_percent`) und an jeder Messung (`readings.soc_sigma_percent`).

Der erste Wert eines Geräts ohne Vorgeschichte kommt allein aus der Spannung. Unter hoher Last ist er unsicher, weil der
Innenwiderstand nicht bekannt ist (20 mΩ bei 25 A sind 0,5 V). Die Unsicherheit eines solchen Startwerts ist auf 30 Punkte
begrenzt: Wer gar nichts weiß und gleichmäßig zwischen 0 und 100 Prozent rät, hat etwa 29 Punkte. Danach schrumpft sie mit
jeder weiteren Messung.

Auch die Meldungen tragen die Spanne. Die E-Mail zeigt neben dem Ladestand "± 3 Punkte" und bei der Restlaufzeit die Spanne
("etwa 14 bis 31 Std."), der Kurztext für WhatsApp und Telegram nennt "Ladestand 12 % (± 3)".
## Spannungskurve

Die Kurve (`OCV_CURVE`) ist keine Lehrbuchkurve, sondern gemessen: Gerät 101 der Messdatenplattform hat bei 285 mA eine
Batterie vollständig entladen (14 Tage, 12,7 V bis 3,9 V, 1315 Messwerte im Abstand von 15 Minuten). Mit der Trapezregel
der Vorlesung und 80 Ah wurde jeder Messwert einem Ladezustand zugeordnet und je 5-Prozent-Stufe gemittelt.

| Ladezustand | 100 | 87,5 | 72,5 | 52,5 | 32,5 | 22,5 | 12,5 | 7,5 | 2,5 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Ruhespannung in mV | 12720 | 12531 | 12297 | 11952 | 11605 | 11371 | 10756 | 9890 | 8874 |

Unter etwa 15 Prozent bricht die Spannung ein. Die frühere Gerade zwischen 10,5 und 12,7 V überschätzte mittlere Ladestände
um 10 bis 17 Punkte (bei 11,95 V sagte sie 66 Prozent, die Messung zeigt 50). Die Einstellungen `min_voltage_mv` und
`max_voltage_mv` bleiben für die Effizienzbewertung erhalten, der Ladezustand rechnet nicht mehr mit ihnen.

## Annahmen und Parameter

| Größe | Wert | Herkunft |
| --- | --- | --- |
| Innenwiderstand | 15 mΩ ± 20 mΩ | Annahme für eine 80-Ah-Bleibatterie, nicht gemessen. Der Spannungsabfall unter Last wird herausgerechnet, die Unsicherheit geht in die Güte der Messung ein |
| Rauschen der Spannungsmessung | 5 mV | gemessen: rund 3 mV (Gerät 101), mit Reserve |
| Temperatur | 1,2 mV je Kelvin, Bezug 20 Grad | Literaturwert für eine 12-V-Bleibatterie. Die echten Reihen haben keine Temperaturmessung, dann ± 10 Kelvin Unsicherheit |
| Zufälliger Kurvenfehler je Messung | 1 Prozentpunkt | Annahme |
| Fester Kurvenfehler | 3 Prozentpunkte | in der simulierten Welt eingestellt (2 Punkte reichten bei schwerer Abweichung nicht, Abdeckung 81 Prozent) |
| Anfangsunsicherheit Strom-Offset / Faktor | 15 mA / 5 Prozent | Annahme |
| Zuschlag beim Start aus der Spannung | 4 Punkte, höchstens 30 | TN0001: erste Messung 6 Punkte über allen späteren |
| Lücke | ab 30 Minuten | Strom unbekannt, 50 Prozent Unsicherheit (Annahme) |
| Ausreißer | 4 σ, Neubeginn nach 12 | Annahme |
| Selbstkontrolle | Mittel über 50 Messungen, Spanne mal √fit ab fit über 1 | verbreitert die Spanne, wenn das Modell dauerhaft danebenliegt |

Alle Werte stehen als benannte Konstanten am Anfang von `estimator.rs`.

## Statusregeln

Kritisch und Warnung entscheiden mit dem vorsichtigen Wert, also der Schätzung minus einer Standardabweichung
(`STATUS_SIGMAS`). Je unsicherer ein Ladezustand, desto früher wird gewarnt: 45 Prozent bei ± 2 sind in Ordnung, 45
Prozent bei ± 8 sind eine Warnung. Ohne Angabe der Unsicherheit gilt die Schätzung selbst. Die Ladestand-Anzeige und die
Schwellwerte bleiben unverändert, nur die Entscheidung ist vorsichtiger.

## Prognose mit Spanne

Die Entladerate kommt wie bisher aus der Ausgleichsgeraden über die letzten 24 Stunden (seit dem letzten Batteriewechsel).
Neu ist ihr Standardfehler (aus Bestimmtheitsmaß und Streuung) und ein Zuschlag von 10 Prozent der Rate für Lastschwankungen
der Zukunft. Mit der Unsicherheit des Ladezustands ergibt die Fehlerfortpflanzung erster Ordnung eine Spanne:

```
T = s / r          σ_T² = (σ_s / r)² + (s · σ_r / r²)²
vorsichtig = T − 1,28 · σ_T      optimistisch = T + 1,28 · σ_T
```

Die Spanne gilt mit etwa 80 Prozent Wahrscheinlichkeit (Test: 20 000 Ziehungen). Angezeigt wird sie in Klammern hinter der
Zeit bis leer ("Leer in ca. 25 Std. (18 bis 36 Std.)") und im Verlauf als Trichter.

## Prüfung

**Simulierte Welt mit bekannter Wahrheit** (`estimator::tests`, 80 Batterien je Fall, fester Zufall). Die Batterien
haben alle Fehler, die das Filter nicht kennt: abweichende Kapazität, Sensor-Offset und Verstärkungsfehler, versetzte und
geneigte Kennlinie, anderer Innenwiderstand und Temperaturbeiwert, Datenlücken von 2 bis 40 Stunden. "Schwere 2" verdoppelt
alle Abweichungen (alte, schlecht kalibrierte Batterie).

| Fall | Abdeckung der 95-%-Spanne | mittlerer Fehler | mittlere Spanne | altes Verfahren |
| --- | --- | --- | --- | --- |
| gewöhnliche Batterie, ohne Lücken | 100 % | 1,5 Punkte | ± 5,9 | 7,3 Punkte |
| gewöhnliche Batterie, mit Lücken | 100 % | 1,5 Punkte | ± 5,9 | 7,3 Punkte |
| schwere Abweichung, ohne Lücken | 98,0 % | 3,0 Punkte | ± 5,9 | 8,2 Punkte |
| schwere Abweichung, mit Lücken | 97,9 % | 3,0 Punkte | ± 5,9 | 8,1 Punkte |

Die Tests verlangen mindestens 95 Prozent Abdeckung, eine mittlere Spanne unter ± 7 Punkten und weniger als halb so viel
Fehler wie das alte Verfahren. Ein Test mit einem festen Kurvenfehler von nur 2 statt 3 Punkten zeigte vorher eine
Abdeckung von 81 Prozent bei schwerer Abweichung. Daraus folgt der feste Anteil von 3 Punkten.

**Echte Messreihen** (Gerät 101: 1315 Werte in 14 Tagen, volle Entladung; TN0001: 126 Werte in 10 Stunden; beide ohne
Temperaturmessung). Das Abspielwerkzeug liegt in `crates/shared/tests/replay_real.rs`. Gemessen wurde:

- Das Rauschen der Spannung beträgt rund 3 mV, im Modell stehen 5 mV (vorher 15 mV, ohne Messgrundlage).
- Die Selbstkontrolle (NIS, Ziel höchstens 1) liegt bei 0,03 (Gerät 101) und 0,12 (TN0001), es gibt keine Abweisung und
  keinen Neustart. Die Annahmen sind auf diesen Reihen also eher zu vorsichtig als zu zuversichtlich. Gerät 101 ist dabei
  nicht unabhängig, die Kurve stammt aus dieser Entladung (Fehler 0,3 Punkte, ein Konsistenztest).
- TN0001 ist von der Kurve unabhängig (126 Messwerte, 12,296 V bei 311 mA, 72,7 Prozent mit ± 3,0 Standardabweichung, 95
  Prozent zwischen 67 und 79). Die frühere Geraden-Rechnung hätte etwa 82 Prozent gemeldet. Ab der zweiten Messung stimmen
  Integration und Spannung auf 0,8 Punkte überein: 75,7 Prozent nach der Spannung zu Beginn, minus 3,75 Punkte aus 3,0 Ah,
  ergibt 72,0 Prozent, die Spannung am Ende zeigt 72,7 Prozent.
- Die erste Messung von TN0001 lag 6 Punkte über allen späteren (frisch geladene Batterie mit Oberflächenladung). Deshalb
  startet das Filter mit einem Zuschlag und nimmt die erste Messung nicht für bare Münze.

Grenze: Das sind zwei Geräte, eine Batterieart und konstante Last (rund 290 mA). Es reicht, um grobe Fehlannahmen
auszuschließen, nicht, um die Spanne enger zu machen. Deshalb wurde nichts auf diese Reihen hin enger gestellt, außer dem
überhöhten Rauschen der Spannung.

## Grenzen und nächste Schritte

- **Eine einzige Kurve.** Sie stammt von einer Batterie. Der feste Fehler von 3 Punkten deckt typische Abweichungen anderer
  AGM-Batterien ab, sie genauer zu machen heißt, die Kurve je Batterietyp aus einer vollständigen Entladung zu messen. Die
  Kurve ist eine Konstante und lässt sich je Batterietyp aus dem Katalog ersetzen.
- **Keine unabhängige Wahrheit.** Ob die Zahl im Betrieb stimmt, zeigt erst ein zweiter, ebenfalls vollständiger Zyklus oder
  eine Messung am Wechsel (Ladezustand der ausgetauschten Batterie). Beim Wechsel wird der Zustand der alten Batterie
  festgehalten; ein Vergleich mit dem Schätzwert kurz vor dem Wechsel ist der einfachste unabhängige Test.
- **Nur Entladung.** Der Strom zählt als Betrag, Laden (zum Beispiel durch Solarzellen) wird nicht erkannt. Die Spannung
  korrigiert das, aber ein Gerät mit Ladequelle braucht eine Stromrichtung und ein eigenes Modell.
- **Kapazität bei Kälte.** Die Kapazität sinkt bei Kälte (bei 0 Grad etwa 15 bis 20 Prozent). Das Filter lernt den Faktor
  `g` langsam. Eine feste Kältekorrektur gibt es nicht. Im Winter sind die Prognosen deshalb eher zu optimistisch.
- **Spannungshysterese und Erholung** nach starker Last sind nicht modelliert. Bei den kleinen Strömen der Nodes (unter
  1 A) ist der Einfluss gering und steckt in der Unsicherheit der Messung.
- **Prognose bis 0 Prozent.** Real ist ein Node schon bei etwa 10 bis 15 Prozent (10,5 V) nicht mehr betriebsfähig. Die
  Schwellen für Warnung und kritisch liegen deshalb höher, und der Satz unter dem Verlauf nennt auch die Zeit bis kritisch.
