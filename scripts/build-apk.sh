#!/usr/bin/env bash
# Build the Android app.
#
#   scripts/build-apk.sh              # debug
#   scripts/build-apk.sh --release    # signed, for a phone
#
# dx regenerates its gradle project on every build and overwrites anything
# placed there by hand, so the launcher icon and the manifest hardening are
# applied afterwards and gradle is re-run over the patched project.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# shellcheck source=/dev/null
source "$ROOT/scripts/android-env.sh"

PROFILE="debug"
GRADLE_TASK="assembleDebug"
DX_FLAGS=(--target aarch64-linux-android)
for arg in "$@"; do
  if [ "$arg" = "--release" ]; then
    PROFILE="release"
    GRADLE_TASK="assembleRelease"
    DX_FLAGS+=(--release)
  fi
done

for tool in java dx magick; do
  command -v "$tool" >/dev/null || { echo "$tool not found; see docs/android-notes.md" >&2; exit 1; }
done
[ -n "${ANDROID_NDK_HOME:-}" ] || { echo "no NDK under $ANDROID_HOME/ndk" >&2; exit 1; }

BUILD_TOOLS="$ANDROID_HOME/build-tools/35.0.0"
GRADLE_DIR="$ROOT/target/dx/freqtrade/$PROFILE/android/app"

# Start from a clean scaffold. dx regenerates its stock launcher icons every
# build but does not remove the ones we added last time, and gradle refuses to
# merge a .png and a .webp claiming the same resource name — so the build would
# succeed the first time and fail the second.
rm -rf "$GRADLE_DIR"

echo "==> building ($PROFILE, aarch64)"
dx build --package freqtrade --platform android "${DX_FLAGS[@]}"

RES="$GRADLE_DIR/app/src/main/res"
[ -d "$RES" ] || { echo "no gradle project at $GRADLE_DIR" >&2; exit 1; }

echo "==> applying launcher icon and backup rules"
[ -d "$ROOT/crates/freqtrade/android-res" ] || "$ROOT/scripts/gen-android-icons.sh"
# Drop the generated stock icon: it ships vector drawables that would otherwise
# win over ours on Android 8+.
rm -f "$RES"/mipmap-anydpi-v26/ic_launcher*.xml
rm -f "$RES"/drawable*/ic_launcher_*.xml
rm -f "$RES"/mipmap-*/ic_launcher*.webp
cp -r "$ROOT/crates/freqtrade/android-res/." "$RES/"

# Android backs an app's private files up to the cloud by default, which would
# carry the credential key and the database off the device.
python3 "$ROOT/scripts/harden-manifest.py" "$GRADLE_DIR/app/src/main/AndroidManifest.xml"

# dx hardcodes versionCode = 1, so every release looks like the same build to
# Android's package manager and an update can be refused as "already
# installed". Derive it from the workspace version: 2.0.1 -> 20001.
GRADLE_BUILD="$GRADLE_DIR/app/build.gradle.kts"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
CODE=$(echo "$VERSION" | awk -F. '{printf "%d", $1*10000 + $2*100 + $3}')
sed -i "s/^\( *versionCode *=\) *1$/\1 $CODE/" "$GRADLE_BUILD"
echo "   version: $VERSION (code $CODE)"

echo "==> repackaging"
( cd "$GRADLE_DIR" && ./gradlew --quiet "$GRADLE_TASK" )

# Pick the output for the profile we asked for. dx compiles the Rust in
# release mode but packages through gradle's *debug* build type, leaving an
# app-debug.apk that a plain `find` picks up first — which is how every build
# so far shipped with android:debuggable="true".
OUTPUTS="$GRADLE_DIR/app/build/outputs/apk/$PROFILE"
APK=$(find "$OUTPUTS" -name "*.apk" 2>/dev/null | head -1)
[ -n "$APK" ] || { echo "no $PROFILE APK under $OUTPUTS" >&2; exit 1; }
OUT="$HOME/freqtrade-$PROFILE-arm64.apk"
KEYSTORE="${FREQTRADE_KEYSTORE:-$HOME/.config/freqtrade/release.jks}"
PASSFILE="${KEYSTORE%.jks}.password"
ALIAS="${FREQTRADE_KEY_ALIAS:-freqtrade}"

if [ "$PROFILE" = "release" ] && [ -f "$KEYSTORE" ] && [ -f "$PASSFILE" ]; then
  echo "==> signing"
  # Align first: apksigner preserves alignment, but zipalign after signing
  # would invalidate the signature.
  "$BUILD_TOOLS/zipalign" -p -f 4 "$APK" "$OUT.aligned"
  rm -f "$OUT" "$OUT.idsig"
  # Passed by environment, not `pass:` (which exposes the password in the
  # process list) and not `file:` (where both reads share one reader, so the
  # second finds end-of-file).
  FT_KS_PASS="$(tr -d '\n' < "$PASSFILE")" \
  "$BUILD_TOOLS/apksigner" sign \
    --ks "$KEYSTORE" --ks-key-alias "$ALIAS" \
    --ks-pass env:FT_KS_PASS --key-pass env:FT_KS_PASS \
    --out "$OUT" "$OUT.aligned"
  rm -f "$OUT.aligned" "$OUT.idsig"
elif [ "$PROFILE" = "release" ]; then
  cp "$APK" "$OUT"
  echo "  note: no keystore at $KEYSTORE, so this is debug-signed and not"
  echo "        suitable for distribution. Run scripts/make-release-key.sh."
else
  cp "$APK" "$OUT"
fi

echo
echo "  $OUT  ($(du -h "$OUT" | cut -f1))"
"$BUILD_TOOLS/apksigner" verify --print-certs "$OUT" 2>/dev/null | grep -m1 "DN:" | sed 's/^/  /' || true
