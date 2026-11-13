#!/usr/bin/env bash
# Messung der Testabdeckung (Vorlesung: Ziel 80 Prozent der Kernlogik).
#
# Gemessen wird der Code mit Logik. Nicht gezählt werden:
#   * main.rs der Dienste (reine Verdrahtung: Umgebung lesen, Dienst starten),
#   * der Testcode selbst,
#   * die Ansichten und die Browser-Anbindung des Frontends (Komponenten, Fetch, EventSource,
#     JavaScript-Brücke), die nur im Browser läuft. Ihre Logik steckt in den reinen Modulen
#     format, filter, chart, inputs, markers, insights, route, search und share, die mitgezählt werden.
#     In den Ansichtsdateien selbst (tour, tickets, overview, device, inbox, login) stehen zusätzlich kleine reine
#     Funktionen mit eigenen Tests (Tourenreihenfolge, Links, Fristen, Meldungsreihenfolge, Hinweistexte).
#
# Benötigt DATABASE_URL und MQTT_HOST (im Dev-Container gesetzt), weil die Integrationstests mitlaufen.
#
#   docker compose -f docker-compose.dev.yml run --rm dev bash tools/coverage.sh
set -euo pipefail

THRESHOLD="${COVERAGE_MIN:-80}"
IGNORE='(/main\.rs|/tests/|frontend/src/(app|api|device|efficiency_view|forward|history|inbox|jsutil|login|maintenance|map_view|overlays|overview|pins|sse|state|swap|swipe|tickets|tour)\.rs)'

cargo llvm-cov --workspace --locked --summary-only \
  --ignore-filename-regex "$IGNORE" \
  --fail-under-lines "$THRESHOLD"
