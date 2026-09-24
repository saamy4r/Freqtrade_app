#!/usr/bin/env bash
# Development loop: API server plus the web UI with hot reload.
#
#   scripts/dev.sh
#
# Starts ft-server on :3000, then `dx serve` on :8080 with /api proxied to it,
# so the browser sees one origin -- the same arrangement as the shipped build,
# where ft-server serves the UI itself.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

API_PORT="${API_PORT:-3000}"
UI_PORT="${UI_PORT:-8080}"
DB="${FT_DB:-$HOME/.local/share/freqtrade-visualizer/ft.db}"

if ! command -v dx >/dev/null; then
  echo "dx is not installed. Run: cargo install dioxus-cli@0.7.10 --locked" >&2
  exit 1
fi

echo "==> building ft-server"
cargo build -q -p ft-server

echo "==> ft-server on :$API_PORT (db $DB)"
RUST_LOG="${RUST_LOG:-ft_server=debug,ft_client=info,tower_http=warn}" \
  "$ROOT/target/debug/ft-server" --dev --port "$API_PORT" --db "$DB" &
SERVER=$!
trap 'kill "$SERVER" 2>/dev/null || true' EXIT

for _ in $(seq 60); do
  curl -fsS "http://127.0.0.1:$API_PORT/api/health" >/dev/null 2>&1 && break
  sleep 0.1
done

echo "==> dx serve on :$UI_PORT"
echo
echo "    open http://localhost:$UI_PORT"
echo
# No FT_API_BASE: Dioxus.toml proxies /api to the server above, so the browser
# sees a single origin -- the same arrangement as the shipped build.
dx serve --package ft-ui --platform web --port "$UI_PORT"
