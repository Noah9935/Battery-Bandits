#!/usr/bin/env bash
# Durchstich gegen einen laufenden Stack (deploy/start-local): Simulator liefert Messwerte,
# sie erscheinen in Übersicht, Geräteliste, Verlauf und CSV, die Weboberfläche wird ausgeliefert
# und die Echtzeit-Schnittstelle (SSE) streamt durch den Caddy.
#
#   bash tools/smoke.sh http://localhost:8080
set -euo pipefail

BASE="${1:-http://localhost:8080}"
fail() { echo "FEHLER: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }

# Anmeldung (Konten aus deploy/.env.local.example). Der Stack startet noch, deshalb ein paar Versuche.
JAR=$(mktemp)
for _ in $(seq 1 30); do
  curl -fsS -c "$JAR" -H 'content-type: application/json' \
    -d "{\"username\":\"${SMOKE_USER:-noah}\",\"password\":\"${SMOKE_PASSWORD:-noah}\"}" "$BASE/api/auth/login" >/dev/null 2>&1 && break
  sleep 3
done
grep -q bb_session "$JAR" || fail "Anmeldung fehlgeschlagen"
ok "Anmeldung klappt"
code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/summary")
[ "$code" = "401" ] || fail "ohne Anmeldung muss die Schnittstelle 401 liefern, war $code"
ok "ohne Anmeldung kein Zugriff (401)"

# Auf die ersten Messwerte warten (Dienste starten, Simulator sendet alle paar Sekunden).
for _ in $(seq 1 60); do
  total=$(curl -fsS -b "$JAR" "$BASE/api/summary" 2>/dev/null | sed -n 's/.*"total":\([0-9]*\).*/\1/p' || true)
  [ "${total:-0}" -ge 12 ] && break
  sleep 3
done
[ "${total:-0}" -ge 12 ] || fail "nach 3 Minuten nur ${total:-0} Geräte (erwartet 12)"
ok "Übersicht zeigt $total Geräte"

curl -fsS -b "$JAR" "$BASE/api/devices" | grep -q '"id":"SIM01"' || fail "SIM01 fehlt in der Geräteliste"
ok "Geräteliste enthält SIM01"

curl -fsS -b "$JAR" "$BASE/api/devices?status=critical" >/dev/null || fail "Statusfilter"
curl -fsS -b "$JAR" "$BASE/api/devices?sort=soc_percent&dir=asc&attention_first=true" >/dev/null || fail "Sortierung"
ok "Filter und Sortierung antworten"

curl -fsS -b "$JAR" "$BASE/api/devices/SIM01/history?range=24h" | grep -q '"points":\[{' || fail "Verlauf ohne Punkte"
ok "Verlauf liefert Punkte"

headers=$(curl -fsSD - -b "$JAR" -o /tmp/smoke.csv "$BASE/api/devices/SIM01/export.csv?range=24h")
echo "$headers" | grep -qi 'content-disposition: attachment; filename="SIM01_24h.csv"' || fail "CSV ohne Download-Kopfzeile"
head -1 /tmp/smoke.csv | grep -q '^time,device_id,' || fail "CSV ohne Kopfzeile"
ok "CSV-Export mit Content-Disposition"

curl -fsS -b "$JAR" "$BASE/api/settings" | grep -q '"threshold_critical_pct"' || fail "Einstellungen"
ok "Einstellungen lesbar"

curl -fsS -b "$JAR" "$BASE/api/alerts" >/dev/null || fail "Meldungen über den Proxy"
curl -fsS -b "$JAR" "$BASE/api/maintenance" >/dev/null || fail "Wartung über den Proxy"
ok "Proxy zum Meldungs- und Einsatz-Service"

curl -fsS -b "$JAR" "$BASE/" | grep -qi '<html' || fail "Weboberfläche wird nicht ausgeliefert"
curl -fsS -b "$JAR" "$BASE/irgendeine/seite" | grep -qi '<html' || fail "index.html als Rückfall fehlt"
ok "Weboberfläche wird ausgeliefert"

# SSE: Innerhalb von 30 Sekunden muss mindestens ein Ereignis durch den Caddy kommen (Anforderung: unter 30 s).
#   grep beendet die Pipe nach dem ersten Treffer, curl meldet dann einen Schreibfehler (Exit 23). Das ist gewollt.
if { timeout 30 curl -fsSN -b "$JAR" "$BASE/api/events" 2>/dev/null || true; } | grep -m1 -q '^data:'; then
  ok "Echtzeit-Ereignis (SSE) kommt an"
else
  fail "kein SSE-Ereignis innerhalb von 30 Sekunden"
fi

echo "Alle Prüfungen bestanden."
