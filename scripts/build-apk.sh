#!/usr/bin/env bash
# Build the Android app.
#
#   scripts/build-apk.sh              # debug, installs to a connected device
#   scripts/build-apk.sh --release    # release build
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=/dev/null
source "$ROOT/scripts/android-env.sh"

for tool in java sdkmanager dx; do
  command -v "$tool" >/dev/null || { echo "$tool not found; see docs/android-notes.md" >&2; exit 1; }
done
[ -n "${ANDROID_NDK_HOME:-}" ] || { echo "no NDK under $ANDROID_HOME/ndk" >&2; exit 1; }

echo "  JDK  $(java -version 2>&1 | head -1)"
echo "  NDK  $ANDROID_NDK_HOME"
echo

exec dx build --package ft-ui --platform android "$@"
