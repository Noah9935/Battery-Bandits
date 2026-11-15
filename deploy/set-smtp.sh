#!/usr/bin/env bash
# Trägt den Zugang zum Mailserver in die .env ein, damit der Server Meldungen per E-Mail weiterleiten kann.
# Das Passwort wird unsichtbar abgefragt und steht danach nur in der .env: nicht im Chat, nicht im Repository,
# nicht in der Shell-Historie und nicht in einem Protokoll.
#
# Aufruf auf dem Server, im Ordner mit der docker-compose.yml (zum Beispiel ~/battery-bandits/staging/src/deploy):
#   bash set-smtp.sh [Pfad-zur-.env]
# Ohne Pfad gilt die .env zwei Ordner darüber (../../.env), so wie sie auch beim Start der Dienste benutzt wird.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
env_file="${1:-$here/../../.env}"
[ -f "$env_file" ] || { echo "Die Datei $env_file gibt es nicht. Pfad als Argument angeben." >&2; exit 1; }

# Eingaben kommen vom Terminal. Zum Testen lässt sich eine Datei mit drei Zeilen angeben (SET_SMTP_INPUT).
exec 3<"${SET_SMTP_INPUT:-/dev/tty}"

read -r -u 3 -p "Mailserver [mail.racore.net]: " host
host="${host:-mail.racore.net}"
read -r -u 3 -p "Postfach, zum Beispiel alarm@racore.net: " user
[ -n "$user" ] || { echo "Das Postfach fehlt." >&2; exit 1; }
case "$user" in
  *@*) ;;
  *) echo "Das Postfach braucht die ganze Adresse mit @." >&2; exit 1 ;;
esac
read -r -s -u 3 -p "Passwort (bleibt unsichtbar): " pass
echo
[ -n "$pass" ] || { echo "Das Passwort fehlt." >&2; exit 1; }

# Prozentkodierung, damit @, :, / und andere Zeichen die Adresse nicht zerreißen.
urlencode() {
  local LC_ALL=C s="$1" i c out=""
  for ((i = 0; i < ${#s}; i++)); do
    c="${s:i:1}"
    case "$c" in
      [a-zA-Z0-9.~_-]) out+="$c" ;;
      *) out+="$(printf '%%%02X' $(( $(printf '%d' "'$c") & 255 )))" ;;
    esac
  done
  printf '%s' "$out"
}

smtp_url="smtps://$(urlencode "$user"):$(urlencode "$pass")@${host}:465"
unset pass

# Alte Einträge ersetzen, den Rest der .env unverändert lassen. Die Datei bleibt dieselbe (Rechte, Besitzer).
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
chmod 600 "$tmp"
grep -v -E '^(SMTP_URL|SMTP_FROM)=' "$env_file" > "$tmp" || true
printf 'SMTP_URL=%s\nSMTP_FROM=%s\n' "$smtp_url" "$user" >> "$tmp"
cat "$tmp" > "$env_file"
unset smtp_url
echo "SMTP_URL und SMTP_FROM stehen jetzt in $env_file."

if [ "${SET_SMTP_NO_RESTART:-}" = "1" ]; then
  exit 0
fi

# Nur den Dienst neu starten, der die Mails sendet. Die Datenbank und die übrigen Dienste bleiben unberührt.
cd "$here"
docker compose --env-file "$env_file" up -d operations
sleep 3
echo "Der Dienst meldet:"
docker compose --env-file "$env_file" logs --tail 40 operations 2>&1 | grep -i "Weiterleitung" | tail -1 || true
echo "Bei email: true ist der Versand eingerichtet. Jetzt im Postfach die Adresse eintragen und die Probenachricht senden."
