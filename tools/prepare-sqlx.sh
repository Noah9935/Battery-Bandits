#!/usr/bin/env bash
# Erzeugt das Verzeichnis .sqlx/ für SQLx.
#
# SQLx prüft alle SQL-Abfragen beim Kompilieren gegen die Datenbank. Damit auch
# Docker- und CI-Builds ohne Datenbank laufen, werden die Ergebnisse dieser
# Prüfung in .sqlx/ abgelegt und mit eingecheckt. Das Skript nach jeder Änderung
# an einer Abfrage oder Migration erneut ausführen:
#
#   docker compose -f docker-compose.yml -f docker-compose.dev.yml run --rm dev bash tools/prepare-sqlx.sh
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL fehlt}"
cd "$(dirname "$0")/.."

# Jeder Dienst besitzt ein eigenes Schema und migriert nur dieses.
for service in ingest monitor incidents; do
  echo "== Migration $service"
  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -q -c "create schema if not exists ${service}"
  sqlx migrate run \
    --source "${service}/migrations" \
    --database-url "${DATABASE_URL}?options=-c%20search_path%3D${service}%2Cpublic"
done

echo "== Abfragen prüfen und .sqlx erzeugen"
rm -rf .sqlx
cargo sqlx prepare --workspace -- --all-targets
echo "fertig: $(ls .sqlx | wc -l) Abfragen in .sqlx"
