#!/usr/bin/env bash
# Startet alles lokal: Datenbank, Mosquitto, vier Dienste, Simulator und Weboberfläche.
# Benötigt nur Docker. Danach im Browser: http://localhost:8080
set -euo pipefail
cd "$(dirname "$0")"

docker compose --env-file .env.local.example --profile local up --build -d "$@"
echo
echo "Battery Bandits läuft: http://localhost:8080"
echo "Protokolle:  docker compose --env-file .env.local.example --profile local logs -f"
echo "Stoppen:     docker compose --env-file .env.local.example --profile local down"
