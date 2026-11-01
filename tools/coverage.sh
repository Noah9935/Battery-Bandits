#!/usr/bin/env bash
# Misst die Testabdeckung der Kernlogik (NF05: mindestens 70 Prozent).
#
#   docker compose -f docker-compose.yml -f docker-compose.dev.yml run --rm dev bash tools/coverage.sh
#
# Zur Kernlogik zählt alles, was ohne Datenbank, Broker und Browser testbar ist:
# Nachrichtenprüfung, Statusbewertung, Ereignisse, API-Typen, das Beobachter-Muster,
# die Flottensimulation sowie Filtern und Formatieren in der Web-App.
#
# Nicht gemessen werden Dateien, die vor allem Ein- und Ausgabe anbinden (Startcode,
# MQTT, NATS, Datenbankzugriffe, HTTP-Handler, die Oberfläche selbst). Sie werden vom
# End-to-End-Test (tools/e2e.sh) gegen den laufenden Stack geprüft.
set -euo pipefail
cd "$(dirname "$0")/.."

IO_FILES='(/main|/receiver|/store|/bus|/db|/handlers|/queries|/engine|/config|/state|/retry|/shutdown|/env)\.rs|api/src/lib\.rs|kit/src/lib\.rs|web/src/app/'
MINIMUM="${MIN_COVERAGE:-70}"

cargo llvm-cov --workspace --ignore-filename-regex "$IO_FILES" --fail-under-lines "$MINIMUM" --summary-only
echo "Abdeckung der Kernlogik: mindestens $MINIMUM Prozent der Zeilen erreicht."
