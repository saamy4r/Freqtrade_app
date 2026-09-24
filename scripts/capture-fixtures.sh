#!/usr/bin/env bash
# Capture real Freqtrade responses as test fixtures.
#
#   scripts/capture-fixtures.sh http://192.168.1.10:8080 myuser mypass
#
# Writes to crates/ft-types/tests/fixtures/live/, which is gitignored: these are
# your real balances and trades, not something to commit. `cargo test -p ft-types`
# picks them up automatically and asserts every one deserializes.
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "usage: $0 <bot-url> <username> <password>" >&2
  echo "  bot-url e.g. http://192.168.1.10:8080  (with or without /api/v1)" >&2
  exit 64
fi

RAW_URL="${1%/}"
USERNAME="$2"
PASSWORD="$3"

# Match the Flutter app's normalization: strip trailing slash, ensure /api/v1.
BASE="$RAW_URL"
[[ "$BASE" == */api/v1 ]] || BASE="$BASE/api/v1"

OUT="$(cd "$(dirname "$0")/.." && pwd)/crates/ft-types/tests/fixtures/live"
mkdir -p "$OUT"

echo "==> $BASE"

# /ping is the only unauthenticated endpoint; use it to fail fast on a bad URL.
if ! curl -fsS --max-time 5 "$BASE/ping" -o "$OUT/ping.json"; then
  echo "ping failed -- is the bot reachable and is the API enabled?" >&2
  exit 1
fi
echo "    ping            $(cat "$OUT/ping.json")"

TOKEN_JSON="$(curl -fsS --max-time 10 -X POST -u "$USERNAME:$PASSWORD" "$BASE/token/login")" || {
  echo "login failed -- check username/password" >&2
  exit 1
}
printf '%s' "$TOKEN_JSON" > "$OUT/token_login.json"
ACCESS="$(printf '%s' "$TOKEN_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')"
REFRESH="$(printf '%s' "$TOKEN_JSON" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("refresh_token",""))')"
echo "    token_login     access_token ok, refresh_token $([[ -n $REFRESH ]] && echo present || echo MISSING)"

get() { # get <fixture-name> <path>
  local name="$1" path="$2"
  if curl -fsS --max-time 15 -H "Authorization: Bearer $ACCESS" "$BASE$path" \
     | python3 -m json.tool > "$OUT/$name.json" 2>/dev/null; then
    echo "    $(printf '%-15s' "$name") $(wc -c < "$OUT/$name.json") bytes"
  else
    echo "    $(printf '%-15s' "$name") FAILED ($path)" >&2
    rm -f "$OUT/$name.json"
  fi
}

if [[ -n "$REFRESH" ]]; then
  curl -fsS --max-time 10 -X POST -H "Authorization: Bearer $REFRESH" \
    "$BASE/token/refresh" > "$OUT/token_refresh.json" 2>/dev/null \
    && echo "    token_refresh   ok" \
    || echo "    token_refresh   FAILED" >&2
fi

get show_config   /show_config
get status        /status
get trades        "/trades?limit=50&offset=0"
get profit        /profit
get balance       /balance
get logs          "/logs?limit=50"
get whitelist     /whitelist

# Chart data needs a pair and the bot's own timeframe, so derive both.
PAIR="$(python3 -c '
import json,sys
try:
    wl = json.load(open(sys.argv[1]))["whitelist"]
    print(wl[0] if wl else "")
except Exception:
    print("")
' "$OUT/whitelist.json" 2>/dev/null || true)"
TF="$(python3 -c '
import json,sys
try:
    print(json.load(open(sys.argv[1])).get("timeframe") or "5m")
except Exception:
    print("5m")
' "$OUT/show_config.json" 2>/dev/null || echo 5m)"

if [[ -n "$PAIR" ]]; then
  get pair_candles "/pair_candles?pair=$(python3 -c 'import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1], safe=""))' "$PAIR")&timeframe=$TF&limit=100"
  echo "    (candles for $PAIR @ $TF)"
else
  echo "    pair_candles    skipped -- empty whitelist" >&2
fi

echo
echo "Captured to $OUT"
echo "Now run: cargo test -p ft-types"
