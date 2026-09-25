#!/usr/bin/env bash
# Generate the Android launcher icon set from a single square source.
#
#   scripts/gen-android-icons.sh [source.png]
#
# dx generates Android Studio's stock icon and regenerates it on every build,
# so these are kept as checked-in resources and copied over the scaffold by
# build-apk.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:-$ROOT/assets/icon/icon.png}"
RES="$ROOT/crates/freqtrade/android-res"

# The source is a light figure on transparency, so it needs an opaque backdrop.
# Dark matches the app and makes the white robot carry on a home screen.
BG="#16181c"

command -v magick >/dev/null || { echo "ImageMagick is required" >&2; exit 1; }
[ -f "$SRC" ] || { echo "no source icon at $SRC" >&2; exit 1; }

rm -rf "$RES"
mkdir -p "$RES/values" "$RES/mipmap-anydpi-v26"

# Legacy square icons, pre-Android 8. Source composited onto the backdrop.
for spec in mdpi:48 hdpi:72 xhdpi:96 xxhdpi:144 xxxhdpi:192; do
  density="${spec%%:*}"; px="${spec##*:}"
  mkdir -p "$RES/mipmap-$density"
  magick -size "${px}x${px}" "xc:$BG" \
    \( "$SRC" -resize "${px}x${px}" \) -gravity center -composite \
    "$RES/mipmap-$density/ic_launcher.png"
  cp "$RES/mipmap-$density/ic_launcher.png" "$RES/mipmap-$density/ic_launcher_round.png"
done

# Adaptive foreground, Android 8+. The canvas is 108dp but launchers mask it to
# whatever shape they like and may animate it, so only the middle 72dp is
# guaranteed visible — hence the logo occupies two thirds, centred, on
# transparency.
for spec in mdpi:108 hdpi:162 xhdpi:216 xxhdpi:324 xxxhdpi:432; do
  density="${spec%%:*}"; px="${spec##*:}"
  inner=$(( px * 2 / 3 ))
  mkdir -p "$RES/mipmap-$density"
  magick -size "${px}x${px}" xc:none \
    \( "$SRC" -resize "${inner}x${inner}" \) -gravity center -composite \
    "$RES/mipmap-$density/ic_launcher_foreground.png"
done

cat > "$RES/values/ic_launcher_background.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background">$BG</color>
</resources>
XML

for name in ic_launcher ic_launcher_round; do
  cat > "$RES/mipmap-anydpi-v26/$name.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background" />
    <foreground android:drawable="@mipmap/ic_launcher_foreground" />
</adaptive-icon>
XML
done

echo "wrote $(find "$RES" -type f | wc -l) resources to ${RES#"$ROOT/"}"
