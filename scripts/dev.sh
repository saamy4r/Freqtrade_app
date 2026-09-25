#!/usr/bin/env bash
# Development loop: build the UI, then serve it and the API from one origin.
#
#   scripts/dev.sh            # debug build, fast
#   scripts/dev.sh --release  # what actually ships
#
# Deliberately does NOT use `dx serve`. Its hot-reload keeps a stale wasm module
# alive in the browser after a rebuild, which costs hours: the page looks
# correct and simply behaves like code you are no longer running. Serving the
# built bundle from ft-server is also exactly how the app ships, so the dev loop
# and the product share one path.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PROFILE="debug"
DX_FLAGS=()
if [[ "${1:-}" == "--release" ]]; then
  PROFILE="release"
  DX_FLAGS=(--release)
fi

API_PORT="${API_PORT:-3000}"
DB="${FT_DB:-$HOME/.local/share/freqtrade-visualizer/ft.db}"

# Arch's rustup package puts cargo/rustc shims in /usr/bin, so those work
# without ~/.cargo/bin on PATH -- but anything installed by `cargo install`
# lands there and is invisible. Add it ourselves rather than depending on the
# user's shell configuration.
export PATH="$HOME/.cargo/bin:$PATH"

command -v dx >/dev/null || {
  echo "dx not found on PATH or in ~/.cargo/bin." >&2
  echo "Install it with: cargo install dioxus-cli@0.7.10 --locked" >&2
  exit 1
}

echo "==> building UI ($PROFILE)"
dx build --package freqtrade --platform web "${DX_FLAGS[@]}"

BUNDLE="$ROOT/target/dx/freqtrade/$PROFILE/web/public"
[[ -d "$BUNDLE" ]] || { echo "no bundle at $BUNDLE" >&2; exit 1; }

echo "==> building server"
cargo build -q -p ft-server

echo
echo "    http://localhost:$API_PORT"
echo
# A debug bundle uses stable filenames, so the server sends no-store to stop the
# browser pinning an old module. Release builds are content-hashed already.
RUST_LOG="${RUST_LOG:-ft_server=debug,ft_client=info,tower_http=warn}" \
  exec "$ROOT/target/debug/ft-server" --port "$API_PORT" --db "$DB" --ui "$BUNDLE"
