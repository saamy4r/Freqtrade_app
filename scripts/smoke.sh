#!/usr/bin/env bash
# End-to-end smoke test against a live bot.
#
#   scripts/smoke.sh http://192.168.1.10:8080 username password
#
# Starts ft-server on a scratch database, adds the bot through the real API,
# loads every screen, and reports cold vs warm timings. Leaves nothing behind.
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $0 <bot-url> <username> <password>" >&2
  exit 64
fi
URL="$1"; USER="$2"; PASS="$3"
PORT="${PORT:-3999}"
DB="$(mktemp -d)/smoke.db"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cargo build -q -p ft-server
RUST_LOG=ft_server=info "$ROOT/target/debug/ft-server" --port "$PORT" --db "$DB" >/tmp/ft-smoke.log 2>&1 &
SERVER=$!
cleanup() { kill "$SERVER" 2>/dev/null || true; rm -rf "$(dirname "$DB")"; }
trap cleanup EXIT

for _ in $(seq 50); do
  curl -fsS "http://127.0.0.1:$PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.1
done
echo "server up on :$PORT (db $DB)"
echo

# Times one request, printing milliseconds and a jq-extracted summary.
timed() { # timed <label> <url> <jq-filter>
  local label="$1" url="$2" filter="$3"
  local start end body
  start=$(date +%s%N)
  body=$(curl -fsS "$url") || { echo "  $(printf '%-22s' "$label") FAILED"; return 1; }
  end=$(date +%s%N)
  printf '  %-22s %5dms  %s\n' "$label" $(( (end - start) / 1000000 )) "$(echo "$body" | jq -rc "$filter")"
}

echo "==> adding bot"
BOT=$(curl -fsS -X POST "http://127.0.0.1:$PORT/api/bots" \
  -H 'content-type: application/json' \
  -d "$(jq -n --arg u "$URL" --arg n "$USER" --arg p "$PASS" \
        '{name:"live", url:$u, username:$n, password:$p}')")
ID=$(echo "$BOT" | jq -r .id)
echo "  id $ID  url $(echo "$BOT" | jq -r .url)"
echo

B="http://127.0.0.1:$PORT/api/bots/$ID"

echo "==> cold (bot is actually queried)"
timed overview  "$B/overview?refresh=true"  '{open:(.data.open_trades|length), value:.data.portfolio_value, pl:.data.open_pl, stale:.stale}'
timed closed    "$B/closed?refresh=true"    '{total:.data.total, shown:(.data.trades|length), profit:.data.closed_profit}'
timed dashboard "$B/dashboard?refresh=true" '{points:(.data.series.points|length), unit:.data.series.unit, strategy:.data.config.strategy}'
timed logs      "$B/logs?refresh=true"      '{entries:(.data.entries|length)}'
timed pairs     "$B/pairs?refresh=true"     '{pairs:(.data|length), extra:[.data[]|select(.in_whitelist|not)|.pair]}'
PAIR=$(curl -fsS "$B/pairs" | jq -r '.data[0].pair')
timed candles   "$B/candles?pair=$(jq -rn --arg p "$PAIR" '$p|@uri')&refresh=true" '{pair:.data.pair, tf:.data.timeframe, bars:(.data.candles|length)}'
echo

echo "==> warm (served from cache, bot untouched)"
timed overview  "$B/overview"  '{open:(.data.open_trades|length), stale:.stale}'
timed closed    "$B/closed"    '{total:.data.total}'
timed dashboard "$B/dashboard" '{points:(.data.series.points|length)}'
timed logs      "$B/logs"      '{entries:(.data.entries|length)}'
echo

echo "==> incremental sync (second refresh fetches only what is new)"
timed closed    "$B/closed?refresh=true" '{total:.data.total}'
echo
echo "trade requests the server made to the bot:"
grep -o 'incremental trade sync.*' /tmp/ft-smoke.log | tail -3 || echo "  (none logged; already up to date)"
echo
echo "done."
