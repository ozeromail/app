<p align="center">
  <img src="docs/assets/ozero-mark.svg" width="120" alt="">
</p>

<h1 align="center">ozero mail</h1>

<p align="center">A mail client for Android and iOS: one Rust core, a native UI on each platform.</p>

## Layout

```
core/    Rust core: domain → app → api
android/ Android app (Jetpack Compose)
ios/     iOS app (SwiftUI)
i18n/    UI strings: en.json (source), ru.json
tools/   CI checks
docs/    decisions (ADR)
```

## Build

Requires `rustup`; the Rust version is pinned in `rust-toolchain.toml`.

```sh
cd core && cargo test
```

The Android build also needs the Android SDK and NDK and `cargo install cargo-ndk`:

```sh
cd android && ./gradlew :app:assembleDebug
```

Android CI uploads the debug APK as the `ozero-mail-debug` artifact. It is signed with the debug key from the `ANDROID_DEBUG_KEYSTORE` secret (a base64 keystore), so each build installs over the last.

The iOS app needs Xcode 16 and the core built as an XCFramework first:

```sh
tools/build-xcframework.sh
open ios/OzeroMail.xcodeproj
```
