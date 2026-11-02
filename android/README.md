# Ferrowl for Android

Kotlin + Jetpack Compose app on top of the Rust engine (`../core`, bound with
UniFFI in `../ffi`). Architecture follows Google's guide to app architecture:
one Activity, Compose screens, ViewModels exposing `StateFlow`, a repository
(`MailRepository`) in front of the engine, Hilt for dependency injection.

## Modules

- `core/` — the engine as an Android library: `libferrowl_ffi.so` for
  arm64-v8a, armeabi-v7a, x86_64 and the generated Kotlin bindings
  (package `com.ferrowl.core`). Both are produced by `../ffi/build-android.sh`,
  which Gradle runs automatically when the Rust sources change; they are not
  committed.
- `app/` — the app (package `com.ferrowl.app`):
  - `data/` — `MailRepository` (engine calls), `AccountStore` (settings in
    DataStore), `SecretBox` (passwords encrypted with an Android Keystore key).
  - `ui/` — screens (`setup`, `inbox`, `message`), theme (desktop colors
    Ruggine/Notte), type-safe navigation.
  - UI texts are generated at build time from `../core/locales/*.toml`, the
    same files as the desktop app.

## Requirements

Android Studio with, in **SDK Manager**:

- SDK Platforms: **Android SDK Platform 37**
- SDK Tools: **NDK (Side by side)**, **Android SDK Command-line Tools**

Rust: `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`
and `cargo install cargo-ndk`.

## Build

Open the `android/` folder in Android Studio and press Run, or:

```bash
./gradlew :app:assembleDebug
```

## Security notes

- Account passwords are encrypted with an AES key held by the Android Keystore
  (never leaves the device); cloud backup and device transfer are disabled.
- HTML mail is shown in a WebView with JavaScript, file and content access off;
  links open in the browser.
