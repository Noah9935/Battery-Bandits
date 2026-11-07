# UI/UX Art Direction und Anti-KI-Design-System (Linear, Vercel, Braun)

**Vorrang hat das Mockup `docs/TrafficNode_UI_Mockup.html`** (Entscheidung Noah, 07.11.2026: "viel mehr wie bei dem Mockup"). Es legt fest: Schrift Nunito, dunkle Seitenleiste in Maskenfarbe mit Streifen und Zählern, Karten mit 14px Radius und weichem Schatten, getönte Status-Pillen, Kennzahlen mit Farbwerten und Streifenleiste bei Auswahl, Karte mit Filter-Seitenleiste (Suche, Status, Herkunft, Ladestand), Wartung und Optionen als getrennte Seiten. Farben und Radien stehen in `frontend/style.css` (Variablen am Anfang), sie stammen aus dem Mockup.

Die folgenden Regeln gelten weiter, soweit das Mockup nichts anderes zeigt. Typische "KI-Template-Muster" bleiben verboten (zum Beispiel Farbverläufe auf Buttons, Emojis, selbst gemalte Illustrationen, Runden-Inflation, schreiende Farbflächen).

## 1. Verbotene KI-Muster (strikt)
- Schatten nur so zart wie im Mockup (`0 1px 2px`), keine großen Schlagschatten auf normalen Karten. Überlagerungen (Schublade, Palette) dürfen den mittleren Schatten des Mockups nutzen.
- Farbverläufe nur der Streifen des Mockups (Seitenleiste, aktive Kennzahl), nie auf Buttons, Flächen oder Texten.
- Keine bunten Karten-Hintergründe: Farbe zeigt sich im Zahlenwert und in kleinen getönten Pillen, wie im Mockup.
- KEINE Emojis im UI und KEINE selbst erfundenen Freihand-SVG-Illustrationen. Nutze ausschließlich präzise 1.5px Stroke-Icons (Lucide-Stil, lokal unter `frontend/icons`). Das Logo aus `assets/logo` ist vorgegeben.
- Radien wie im Mockup: Karten und Tabellen 14px, Buttons, Eingaben und Menüs 10px, Chips und Pillen rund.

## 2. Visuelle Architektur (Swiss Grid und Industrial Minimalism)
- **Flächen und Konturen (Surface Layering):**
  - Arbeite mit flachen Ebenen und gestochen scharfen `1px solid`-Rahmen.
  - **Waschbär-Palette aus dem Mockup:** hell Canvas `#F3F0EB`, Panel `#FFFFFF`, Maske `#2A272D`, Text `#26232A`, Akzent Kupfer `#B87428`; dunkel Canvas `#131215`, Panel `#1C1A1F`, Maske `#0D0C0F`, Text `#ECE6DE`, Akzent `#E3A55B`. Status Grün, Gelb, Rot, Grau mit getönten Hintergründen.
  - Zusätzlich schaltbar (Optionen): Farbschema automatisch, hell oder dunkel, hoher Kontrast, kompakte Tabelle.
- **Typografie und Daten-Präzision:**
  - Schrift: `'Nunito'` (lokal unter `frontend/vendor/fonts`) für die ganze Oberfläche.
  - Alle numerischen Werte zwingend mit `font-variant-numeric: tabular-nums`.
  - Überschriften: `font-weight: 600`, `letter-spacing: -0.02em`, keine Versalien.
  - Tabellen-Header: `11px`, `font-weight: 500`, `color: var(--muted)`, `letter-spacing: 0.04em`.
- **Datendichte und Weißraum:**
  - 4px/8px-Abstandsraster (`8px`, `12px`, `16px`, `24px`, `32px`).
  - Statt vieler kleiner Boxen mit Rahmen: durchgehende horizontale Haarlinien (`1px solid var(--border)`) und großzügiger Weißraum.
  - Status-Anzeigen bestehen aus einem 6px großen farbigen Punkt (`border-radius: 50%`) plus klarem Text in normaler Textfarbe, keine grellen Vollflächen-Pills. Rot, Gelb und Grün nie als großflächiger Hintergrund.
- **Micro-Interactions:**
  - Dezente Übergänge (`transition: background-color 120ms ease, border-color 120ms ease`).
  - Tabellenzeilen bekommen beim Hover nur eine minimale Hintergrund-Aufhellung, keine Verschiebung (`transform: translateY` ist verboten).

## 3. Verifikation
- Prüfe nach jeder CSS- oder Komponenten-Änderung im Browser (Playwright MCP, eingebauter Browser oder lokaler Server), ob Abstände, Textbrüche und Kontraste auf 1440px (Desktop) und 390px (Mobile) bündig sitzen, jeweils im hellen und im dunklen Schema.
