# Android cross-compilation notes

Running list of hazards found while building, so M11 (Android toolchain + first APK)
is not a surprise. Updated as we go.

## Toolchain (installed at M11)

Everything is under `$HOME` — no system packages, no sudo. That was deliberate:
this machine's pacman setup has a single Omarchy mirror that served a corrupt
signature for `rustup`, so relying on it for a multi-gigabyte SDK was not
appealing. `source scripts/android-env.sh` sets it all up.

| Piece | Where | How |
|---|---|---|
| rustup | `~/.rustup` | pacman `-U` from the package cache (see below) |
| Rust targets | — | `rustup target add aarch64-linux-android x86_64-linux-android` |
| JDK 17 | `~/.local/share/jdk17` | Temurin tarball from the Adoptium API |
| SDK tools | `~/Android/Sdk/cmdline-tools/latest` | `commandlinetools-linux-*.zip` from dl.google.com |
| Platform, build-tools, NDK | `~/Android/Sdk` | `sdkmanager --install` |

Versions in use: JDK 17.0.20.1 (Temurin), platform `android-35`, build-tools
`35.0.0`, NDK `27.2.12479018`.

rustup itself was the one piece that needed root, and the mirror would not serve
its signature. It was installed from the already-cached package instead:
`sudo pacman -U /var/cache/pacman/pkg/rustup-*.pkg.tar.zst`, which works because
`pacman.conf` has `LocalFileSigLevel = Optional`.

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

## Browser caching of the wasm bundle

A debug `dx build` emits `wasm/ft-ui_bg.wasm` — a stable filename. A browser
will cache that indefinitely and keep executing the previous build after a
rebuild, which is a uniquely expensive failure: the page renders fine and
simply behaves like code you are no longer running, so every experiment you run
against it is meaningless.

Two mitigations, both in place:

- `ft-server` sends `Cache-Control: no-store, must-revalidate` for everything it
  serves from `--ui`. It only ever serves loopback or localhost, so there is
  nothing to gain from caching.
- Release builds are content-hashed (`assets/ft-ui_bg-dxh5e48….wasm`), so the
  problem cannot occur there at all.

`dx serve`'s hot-reload has the same hazard plus its own: a non-hot-reloadable
change leaves an "app is being rebuilt" overlay while still serving the old
module. `scripts/dev.sh` avoids it entirely by building the bundle and serving
it from `ft-server`, which is also how the app ships.

A related note for diagnosing UI problems: a CDP screenshot timing out
("renderer may be frozen") is not on its own evidence of a hang. During this
work the page executed injected JavaScript and responded correctly while
screenshots timed out. Probe with `javascript_tool` before concluding anything
is stuck.

## Dioxus resource dependencies

`use_resource`'s dependencies are the signals read while the *closure* runs, not
inside the future it returns. Two rules follow, and both were violated in M5:

- Read signals in the closure body. A dependency captured as a plain clone from
  outside never re-triggers the resource.
- Never read a signal inside the `async` block if the resource's own completion
  can change it. That subscribes the resource to itself and it restarts forever.

## First APK (M11)

Built with `scripts/build-apk.sh`. Release, arm64:

| | size |
|---|---|
| Flutter APK v1.0.0 | 50 MB |
| Rust APK (release, `aarch64`) | **9.5 MB** |

Inside the 9.5 MB: `lib/arm64-v8a/libmain.so` is 7.9 MB and contains the whole
application — UI, embedded Axum server, SQLite, rustls, AES-GCM — plus 12 MB of
`classes*.dex` before compression. No Flutter engine, and the WebView is
supplied by the system rather than bundled.

Two things to know when building:

- `dx build --platform android` defaults to **x86_64** (the emulator). Pass
  `--target aarch64-linux-android` for a real phone, or the APK will install
  and then fail to start.
- The release APK is still written to a path ending `apk/debug/app-debug.apk`.
  That is dx's gradle scaffold naming, not a debug build; the contents differ
  (9.5 MB release vs 106 MB debug, whose unstripped `.so` alone is 385 MB).

The M0 dependency choices all held up under cross-compilation, first try:
`rusqlite` with bundled SQLite C, `ring` rather than `aws-lc-rs`, and `aes-gcm`
instead of SQLCipher. None of them needed NDK coaxing.

## Emulator

`avdmanager` and the emulator disagree about where AVDs live. The command line
tools from 2023 cannot read the android-35 package metadata at all — they fail
with "This version only understands SDK XML versions up to 3 but an SDK XML
file of version 4 was encountered", and silently list no system images, so AVD
creation appears to succeed and produces nothing.

After `sdkmanager --install "cmdline-tools;latest"` (installed alongside the
old one as `latest-2`, so swap it into place), a newer `avdmanager` works — but
writes the AVD to `$XDG_CONFIG_HOME/.android/avd` while the emulator only looks
in `~/.android/avd`. Move `ft-test.avd` and `ft-test.ini` there and fix the
absolute `path=` inside the `.ini`.

Running it:

```
emulator -avd ft-test -no-window -no-audio -no-snapshot -gpu swiftshader_indirect
adb install -r <x86_64 apk>        # the emulator is x86_64, not arm64
adb logcat -s freqtrade:V RustPanic:V
```

A release build is not debuggable, so `adb shell run-as` cannot read its files.
To drive the app instead, forward its embedded server — the port is in the log —
and talk to the API directly:

```
adb forward tcp:9100 tcp:<port>
curl http://127.0.0.1:9100/api/bots
```

From inside the emulator, the host machine is `10.0.2.2`.

## Credential key storage

The database key is wrapped by a non-exportable AES key in the Android
Keystore; only ciphertext reaches disk, and on most phones the wrapping key
lives in a secure element. Verified on Android 15: the key survives a
force-stop, and a stored bot password still decrypts on the next launch.

An existing plaintext key (`ft.db.key`, what desktop still uses) is migrated
into the keystore on first run and then deleted, so upgrading does not lose
saved bots. `FileKey::beside` has a test pinning that filename, because the
migration looks for it by name.

If the keystore is unavailable the app falls back to the file-based key — but
only when no wrapped key exists yet. Falling back after a successful wrap would
generate a different key and strand every saved credential, so that case is a
hard error instead.
