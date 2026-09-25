#!/usr/bin/env bash
# Build the Android app.
#
#   scripts/build-apk.sh              # debug
#   scripts/build-apk.sh --release    # what you install on a phone
#
# dx generates the gradle project fresh on every build, including Android
# Studio's stock launcher icon, and overwrites anything placed there by hand.
# So the icons are applied afterwards and gradle is re-run over the patched
# project.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=/dev/null
source "$ROOT/scripts/android-env.sh"

PROFILE="debug"
DX_FLAGS=(--target aarch64-linux-android)
GRADLE_TASK="assembleDebug"
for arg in "$@"; do
  if [ "$arg" = "--release" ]; then
    PROFILE="release"
    DX_FLAGS+=(--release)
  fi
done

for tool in java dx magick; do
  command -v "$tool" >/dev/null || { echo "$tool not found; see docs/android-notes.md" >&2; exit 1; }
done
[ -n "${ANDROID_NDK_HOME:-}" ] || { echo "no NDK under $ANDROID_HOME/ndk" >&2; exit 1; }

GRADLE_DIR="$ROOT/target/dx/freqtrade/$PROFILE/android/app"

# Start from a clean scaffold. dx regenerates its stock launcher icons on every
# build but does not remove the ones we added last time, and gradle refuses to
# merge a .png and a .webp claiming the same resource name. Leaving the old
# project in place makes the build fail on the second run and succeed on the
# first, which is a miserable thing to debug.
rm -rf "$GRADLE_DIR"

echo "==> building ($PROFILE, aarch64)"
dx build --package freqtrade --platform android "${DX_FLAGS[@]}"
RES="$GRADLE_DIR/app/src/main/res"
[ -d "$RES" ] || { echo "no gradle project at $GRADLE_DIR" >&2; exit 1; }

echo "==> applying launcher icon"
[ -d "$ROOT/crates/freqtrade/android-res" ] || "$ROOT/scripts/gen-android-icons.sh"
# Remove the generated stock icon first: it ships vector drawables that would
# otherwise win over ours on Android 8+.
rm -f "$RES"/mipmap-anydpi-v26/ic_launcher*.xml
rm -f "$RES"/drawable*/ic_launcher_*.xml "$RES"/drawable*/'$ic_launcher_foreground__0.xml'
rm -f "$RES"/mipmap-*/ic_launcher*.webp
cp -r "$ROOT/crates/freqtrade/android-res/." "$RES/"

echo "==> repackaging"
( cd "$GRADLE_DIR" && ./gradlew --quiet "$GRADLE_TASK" )

APK=$(find "$GRADLE_DIR" -name "*.apk" -newer "$RES/values/ic_launcher_background.xml" | head -1)
[ -n "$APK" ] || APK=$(find "$GRADLE_DIR" -name "*.apk" | head -1)
OUT="$HOME/freqtrade-$PROFILE-arm64.apk"
cp "$APK" "$OUT"

echo
echo "  $OUT  ($(du -h "$OUT" | cut -f1))"
