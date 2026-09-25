#!/usr/bin/env bash
# Install the Linux build for the current user.
#
#   scripts/install-linux.sh              # from a local release build
#   scripts/install-linux.sh <tarball>    # from a downloaded release
#   scripts/install-linux.sh --uninstall
#
# Everything goes under $HOME. No root, nothing outside XDG directories.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="freqtrade-visualizer"
PREFIX="$HOME/.local"
# Deliberately ~/.local/lib and not ~/.local/share/$APP: that second path is
# the app's own data directory (see backend::data_dir), so installing there
# would put the binary next to ft.db and --uninstall would delete the bots.
LIBDIR="$PREFIX/lib/$APP"
DATADIR="$PREFIX/share/$APP"
BIN="$PREFIX/bin/$APP"
DESKTOP="$PREFIX/share/applications/$APP.desktop"
ICON="$PREFIX/share/icons/hicolor/512x512/apps/$APP.png"

if [ "${1:-}" = "--uninstall" ]; then
  rm -rf "$LIBDIR"
  rm -f "$BIN" "$DESKTOP" "$ICON"
  command -v update-desktop-database >/dev/null && \
    update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
  echo "Removed. Your bots and cached trades are still in"
  echo "  $DATADIR/ (delete it to start over)"
  exit 0
fi

# Source: an explicit tarball, or the most recent local release build.
if [ $# -ge 1 ]; then
  STAGE=$(mktemp -d)
  tar -xzf "$1" -C "$STAGE"
  SRC=$(find "$STAGE" -name "$APP" -type f -perm -u+x | head -1)
  SRC=$(dirname "$SRC")
else
  SRC="$ROOT/target/dx/freqtrade/release/linux/app"
  [ -f "$SRC/freqtrade" ] || {
    echo "No release build found. Run:" >&2
    echo "  dx build --package freqtrade --platform desktop --release" >&2
    exit 1
  }
fi

BINARY="$SRC/freqtrade"
[ -f "$BINARY" ] || BINARY="$SRC/$APP"
[ -f "$BINARY" ] || { echo "no binary in $SRC" >&2; exit 1; }

mkdir -p "$LIBDIR" "$(dirname "$BIN")" "$(dirname "$DESKTOP")" "$(dirname "$ICON")"

# Named freqtrade-visualizer, not freqtrade: this monitors bots, it is not the
# bot software, and shadowing that on PATH would be a nasty surprise.
install -m 755 "$BINARY" "$LIBDIR/$APP"
ln -sf "$LIBDIR/$APP" "$BIN"

if [ -f "$ROOT/assets/icon/icon.png" ]; then
  install -m 644 "$ROOT/assets/icon/icon.png" "$ICON"
elif [ -f "$SRC/icon.png" ]; then
  install -m 644 "$SRC/icon.png" "$ICON"
fi

cat > "$DESKTOP" <<DESKTOP_EOF
[Desktop Entry]
Type=Application
Name=Freqtrade Visualizer
GenericName=Trading Bot Monitor
Comment=Monitor your Freqtrade bots
Exec=$BIN
Icon=$APP
Terminal=false
Categories=Office;Finance;
Keywords=freqtrade;trading;crypto;bot;
StartupWMClass=freqtrade-visualizer
DESKTOP_EOF

command -v update-desktop-database >/dev/null && \
  update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
  gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" 2>/dev/null || true

echo "Installed:"
echo "  binary   $BIN"
echo "  launcher $DESKTOP"
echo "  icon     $ICON"
echo
echo "Run it with '$APP', or find 'Freqtrade Visualizer' in your app launcher."
