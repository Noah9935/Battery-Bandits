#!/usr/bin/env bash
# End-to-End-Test gegen den laufenden Stack (Profil dev: Simulator und Mosquitto laufen mit).
#
#   docker compose --profile dev up -d --build
#   docker compose -f docker-compose.yml -f docker-compose.dev.yml run --rm dev bash tools/e2e.sh
#
# Geprüft wird der ganze Weg: Simulator, MQTT, Empfang, Überwachung, Meldungen, API, Live-Strom.
set -uo pipefail

BASE="${API_URL:-http://web}/api"
failures=0

ok()   { printf '  ok      %s\n' "$1"; }
fail() { printf '  FEHLER  %s\n' "$1"; failures=$((failures + 1)); }
check() { # check "Beschreibung" "erwartet" "tatsächlich"
  if [ "$2" = "$3" ]; then ok "$1"; else fail "$1 (erwartet: $2, war: $3)"; fi
}
get()  { curl -s -m 10 "$BASE$1"; }
code() { curl -s -m 10 -o /dev/null -w '%{http_code}' "$@"; }

echo "== Warten, bis Geräte da sind"
for i in $(seq 1 60); do
  n=$(get /devices | jq 'length' 2>/dev/null || echo 0)
  [ "${n:-0}" -gt 0 ] && break
  sleep 2
done
[ "${n:-0}" -gt 0 ] && ok "$n Geräte gemeldet" || { fail "Keine Geräte nach 120 s"; exit 1; }

echo "== Gesundheit"
check "Health" "ok" "$(get /health | jq -r .status)"

echo "== Übersicht (F03)"
first=$(get /devices | jq -r '.[0].id')
check "Gerät hat Ladestand zwischen 0 und 100" "true" "$(get /devices | jq '[.[] | .charge_pct >= 0 and .charge_pct <= 100] | all')"
check "Einzelnes Gerät abrufbar" "$first" "$(get "/devices/$first" | jq -r .id)"
check "Unbekanntes Gerät: 404" "404" "$(code "$BASE/devices/GIBT-ES-NICHT")"

echo "== Verlauf (F04)"
for i in $(seq 1 30); do
  points=$(get "/devices/$first/history?range=day" | jq '.points | length')
  [ "${points:-0}" -ge 2 ] && break
  sleep 2
done
[ "${points:-0}" -ge 2 ] && ok "Tagesverlauf hat $points Punkte" || fail "Tagesverlauf hat zu wenige Punkte ($points)"
check "Tagesverlauf in Rohwerten" "raw" "$(get "/devices/$first/history?range=day" | jq -r .resolution)"
check "Jahresverlauf in Stundenwerten" "hourly" "$(get "/devices/$first/history?range=year" | jq -r .resolution)"
check "Monatsverlauf enthält Daten (Echtzeit-Aggregat)" "true" "$(get "/devices/$first/history?range=month" | jq '.points | length > 0')"
check "Ungültiger Zeitraum: 400" "400" "$(code "$BASE/devices/$first/history?range=decade")"

echo "== Kritisch-Markierung (F05) und Schwellwert (NF07)"
check "Schwellwert lesen" "number" "$(get /settings | jq -r '.critical_threshold_pct | type')"
check "Schwellwert 99 setzen" "200" "$(code -X PUT -H 'content-type: application/json' -d '{"critical_threshold_pct":99}' "$BASE/settings")"
sleep 3
check "Bei 99 Prozent sind fast alle Geräte kritisch" "true" "$(get /devices | jq '([.[] | select(.status == "critical")] | length) >= (length * 0.9)')"
check "Schwellwert 1 setzen" "200" "$(code -X PUT -H 'content-type: application/json' -d '{"critical_threshold_pct":1}' "$BASE/settings")"
sleep 3
check "Bei 1 Prozent ist fast kein Gerät kritisch" "true" "$(get /devices | jq '([.[] | select(.status == "critical")] | length) <= (length * 0.1)')"
check "Schwellwert 150 abgelehnt" "400" "$(code -X PUT -H 'content-type: application/json' -d '{"critical_threshold_pct":150}' "$BASE/settings")"
check "Schwellwert 20 zurücksetzen" "200" "$(code -X PUT -H 'content-type: application/json' -d '{"critical_threshold_pct":20}' "$BASE/settings")"
sleep 3

echo "== Meldungen"
check "Offene Meldungen vorhanden" "true" "$(get '/incidents?state=open' | jq 'length > 0')"
id=$(get '/incidents?state=open' | jq -r '.[0].id')
check "Meldung abweisen: 204" "204" "$(code -X POST "$BASE/incidents/$id/dismiss")"
check "Erneut abweisen: 409" "409" "$(code -X POST "$BASE/incidents/$id/dismiss")"
check "Status ist abgewiesen" "dismissed" "$(get '/incidents?state=dismissed' | jq -r --argjson id "$id" '.[] | select(.id == $id) | .state')"
check "Ungültiger Status: 400" "400" "$(code "$BASE/incidents?state=quatsch")"

echo "== Validierung (F06)"
for i in $(seq 1 40); do
  rejected=$(get '/rejected?limit=5' | jq 'length')
  [ "${rejected:-0}" -gt 0 ] && break
  sleep 3
done
[ "${rejected:-0}" -gt 0 ] && ok "Verworfene Nachrichten sind protokolliert" || fail "Keine verworfenen Nachrichten protokolliert"
check "Protokoll nennt einen Grund" "true" "$(get '/rejected?limit=1' | jq '.[0].reason | length > 0')"

echo "== Live-Strom (NF01)"
events=$(curl -s -N -m 15 "$BASE/events" | grep -c '^data:' || true)
[ "${events:-0}" -gt 0 ] && ok "$events Ereignisse in 15 s empfangen" || fail "Keine Live-Ereignisse empfangen"

echo "== Web-App"
check "Startseite" "200" "$(code "${API_URL:-http://web}/")"
check "Unterseite liefert die App (SPA)" "200" "$(code "${API_URL:-http://web}/geraete/$first")"

echo
if [ "$failures" -eq 0 ]; then echo "Alle Prüfungen bestanden."; else echo "$failures Prüfung(en) fehlgeschlagen."; exit 1; fi
