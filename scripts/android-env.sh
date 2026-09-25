#!/usr/bin/env bash
# Environment for Android builds. Source it, do not run it:
#
#   source scripts/android-env.sh
#
# Everything lives under $HOME — no system packages, no sudo. That is
# deliberate: the pacman mirror on this machine serves corrupt signatures, and
# the Android SDK is happier self-contained anyway.

export JAVA_HOME="$HOME/.local/share/jdk17"
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"

# The NDK version is whatever sdkmanager installed; pick it up rather than
# hard-coding, so an upgrade does not silently break the build.
if [ -d "$ANDROID_HOME/ndk" ]; then
  export ANDROID_NDK_HOME="$(find "$ANDROID_HOME/ndk" -maxdepth 1 -mindepth 1 -type d | sort -V | tail -1)"
  export NDK_HOME="$ANDROID_NDK_HOME"
fi

export PATH="$JAVA_HOME/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$HOME/.cargo/bin:$PATH"

if [ -n "${BASH_SOURCE[0]}" ] && [ "${BASH_SOURCE[0]}" = "$0" ]; then
  echo "This script sets environment variables; source it instead:" >&2
  echo "  source scripts/android-env.sh" >&2
  exit 64
fi
