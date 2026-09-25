#!/usr/bin/env bash
# Create the release signing key.
#
#   scripts/make-release-key.sh
#
# ⚠  Back up what this produces and never commit it.
#
# Android identifies an app by its signing key. Every future update must be
# signed with this same key, or phones will refuse to install it as an upgrade
# — users would have to uninstall first, losing their data. There is no
# recovery if it is lost.
set -euo pipefail

KEYSTORE="${FREQTRADE_KEYSTORE:-$HOME/.config/freqtrade/release.jks}"
ALIAS="${FREQTRADE_KEY_ALIAS:-freqtrade}"

# shellcheck source=/dev/null
source "$(cd "$(dirname "$0")" && pwd)/android-env.sh"

if [ -f "$KEYSTORE" ]; then
  echo "A keystore already exists at $KEYSTORE" >&2
  echo "Refusing to overwrite it: replacing a signing key breaks upgrades." >&2
  exit 1
fi

mkdir -p "$(dirname "$KEYSTORE")"
chmod 700 "$(dirname "$KEYSTORE")"

# A generated password, stored beside the keystore with owner-only permissions.
# Both files matter equally; back up the pair.
PASSFILE="${KEYSTORE%.jks}.password"
if [ ! -f "$PASSFILE" ]; then
  # Trailing newline matters: apksigner's `file:` reader expects a line and
  # fails with "end of file reached" without one.
  { head -c 32 /dev/urandom | base64 | tr -d '\n/+=' | head -c 32; printf '\n'; } > "$PASSFILE"
  chmod 600 "$PASSFILE"
fi
PASSWORD="$(tr -d "\n" < "$PASSFILE")"

keytool -genkeypair \
  -keystore "$KEYSTORE" \
  -alias "$ALIAS" \
  -keyalg RSA -keysize 4096 \
  -validity 10000 \
  -storepass "$PASSWORD" -keypass "$PASSWORD" \
  -dname "CN=Freqtrade Visualizer, OU=Unknown, O=Unknown, L=Unknown, ST=Unknown, C=Unknown"
chmod 600 "$KEYSTORE"

echo
echo "  keystore : $KEYSTORE"
echo "  password : $PASSFILE"
echo "  alias    : $ALIAS"
echo
echo "Back up both files somewhere safe. Losing them means no future version"
echo "of this app can ever be installed as an update."
