# UI/UX Art Direction und Anti-KI-Design-System (Linear, Vercel, Braun)

Überarbeite oder erstelle das Frontend-Design strikt nach diesen Design-Engineering-Prinzipien. Jedes typische "KI-Template-Muster" ist verboten.

## 1. Verbotene KI-Muster (strikt)
- KEINE großflächigen Box-Shadows (`box-shadow: 0 10px 30px...`) auf normalen Karten.
- KEINE Farbverläufe (Gradients) auf Buttons, Hintergründen oder Texten.
- KEINE bunten KPI-Karten-Hintergründe oder dicken farbigen `border-left: 5px solid`-Streifen.
- KEINE Emojis im UI und KEINE selbst erfundenen Freihand-SVG-Illustrationen. Nutze ausschließlich präzise 1.5px Stroke-Icons (Lucide-Stil, lokal unter `frontend/icons`). Das Logo aus `assets/logo` ist vorgegeben.
- KEINE Runden-Inflation (`border-radius: 20px` auf allem). Festes Radius-System: Außen-Container `10px`, innere Elemente, Buttons und Inputs `6px`.

## 2. Visuelle Architektur (Swiss Grid und Industrial Minimalism)
- **Flächen und Konturen (Surface Layering):**
  - Arbeite mit flachen Ebenen und gestochen scharfen `1px solid`-Rahmen.
  - **Waschbär-Palette** (aus den Farben des Logos: Maskenanthrazit, Fellcreme, Aschgrau, Kupfer, Blitz-Amber):
    - Hell, "Waschbär bei Tag": Canvas `#EFEAE3`, Primär-Panel `#F8F5F1`, Sekundär-Panel `#E6E0D7`, Border `#D5CEC3`, Text `#26232A`, Muted `#6B6570`.
    - Dunkel, "Waschbär bei Nacht": Canvas `#17151A`, Primär-Panel `#1F1C23`, Sekundär-Panel `#29252F`, Border `#36323C`, Text `#ECE6DE`, Muted `#A39BA3`.
    - Marken-Akzent (sparsam, für Fokus-Ringe und Primär-Aktion): Kupfer `#B87428` (hell) und Blitz-Amber `#E3A55B` (dunkel).
    - Zusätzlich schaltbar: Farbschema automatisch, hell oder dunkel, hoher Kontrast, kompakte Tabelle (alles im Bereich Wartung und Optionen).
- **Typografie und Daten-Präzision:**
  - Schrift: `'Inter', -apple-system, BlinkMacSystemFont, sans-serif` für die Oberfläche, `'JetBrains Mono', ui-monospace, monospace` für Geräte-IDs, Rohdaten (`voltage_mV`, `current_mA`), Koordinaten und Zeitstempel. Beide liegen lokal unter `frontend/vendor/fonts`.
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
