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

A tag like `v0.1.0` also builds the release: a bundle for Play and an APK, signed with the upload key, as the `ozero-mail-v0.1.0` artifact. The tag names the version: `vX.Y.Z`, each part below 100, gives version code `X*10000 + Y*100 + Z`. The key comes from four secrets: `ANDROID_UPLOAD_KEYSTORE` (a base64 keystore), `ANDROID_UPLOAD_KEYSTORE_PASSWORD`, `ANDROID_UPLOAD_KEY_ALIAS` and `ANDROID_UPLOAD_KEY_PASSWORD`. Play signs the app for users with its own key; the upload key only proves an upload is ours.

The iOS app needs Xcode 16 and the core built as an XCFramework first:

```sh
tools/build-xcframework.sh
open ios/OzeroMail.xcodeproj
```
