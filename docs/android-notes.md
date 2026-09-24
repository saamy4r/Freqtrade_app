# Android cross-compilation notes

Running list of hazards found while building, so M11 (Android toolchain + first APK)
is not a surprise. Updated as we go.

## Toolchain not yet present on the dev machine

As of M0 this box has **none** of the Android prerequisites:

- No `rustup` — system Rust is Arch's pacman `rust` 1.98, which ships host std only and
  **cannot** `rustup target add aarch64-linux-android`. Installing rustup is a prerequisite
  for M11 and needs the user's sudo (or the rustup.rs script into `~/.cargo`, which will
  shadow `/usr/bin/cargo` — decide deliberately).
- No JDK (`java`/`javac` absent). Dioxus Android needs JDK 17.
- No Android SDK, NDK, or `adb`. `ANDROID_HOME` / `ANDROID_NDK_HOME` unset.

Present and usable: `cmake`, `ninja`, `unzip`.

M0–M10 are all verifiable in a desktop browser and do not need any of the above.

## TLS stack

Freqtrade bots are usually reached over plain `http://` on a LAN, but https must work too.

- **Crypto provider is pinned to `ring`, not `aws-lc-rs`.** reqwest 0.13's `rustls` feature
  implies `__rustls-aws-lc-rs`; aws-lc-rs needs cmake + a working NDK sysroot and is a
  recurring source of Android cross-compile failures. We instead use
  `reqwest = { features = ["rustls-no-provider", "webpki-roots", ...] }` plus an explicit
  `rustls = { features = ["ring", "std", "tls12"] }`. Verified: `cargo tree` shows `ring`
  and no `aws-lc-rs`.
- **`rustls-platform-verifier` is a hard dependency of reqwest 0.13** — it survives even with
  `rustls-no-provider`. It is Android-aware by design (it reads the system trust store over
  JNI), but it requires its companion Java/AAR artifact to be present in the APK. Expect to
  add that to the gradle config in M11. If it fights back, the escape hatch is to build the
  `reqwest::Client` with webpki roots and an explicitly-constructed `rustls::ClientConfig`.

## Cleartext HTTP

The Flutter `AndroidManifest.xml` declared only `INTERNET`, with no `usesCleartextTraffic`
and no `network-security-config`. Android 9+ blocks cleartext by default, so `http://192.168.x.x:8080`
bots were likely broken on modern devices. M12 must ship a `network-security-config` that
permits cleartext to private address ranges.

## Other

- `rusqlite` uses the `bundled` feature (compiles SQLite from C source). This cross-compiles
  cleanly with the NDK but does mean a C compiler is required on the build host.
- Crypto for credentials is `aes-gcm` (pure Rust) specifically to avoid pulling OpenSSL into
  the Android build; do not switch to SQLCipher.
