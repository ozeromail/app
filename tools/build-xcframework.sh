#!/usr/bin/env bash
# Builds the Rust core as OzeroCore.xcframework (device + simulator) with Swift bindings.
set -euo pipefail

cd "$(dirname "$0")/../core"
out="target/ios"
rm -rf "$out"
mkdir -p "$out/headers" "$out/sim"

cargo build --release -p ozero-api \
    --target aarch64-apple-ios --target aarch64-apple-ios-sim --target x86_64-apple-ios

cargo run -p ozero-bindgen -- generate --language swift \
    --library target/aarch64-apple-ios/release/libozero_api.a --out-dir "$out/swift"

cp "$out/swift/ozero_apiFFI.h" "$out/headers/"
cp "$out/swift/ozero_apiFFI.modulemap" "$out/headers/module.modulemap"

# One simulator slice for both Apple silicon and Intel Macs.
lipo -create target/aarch64-apple-ios-sim/release/libozero_api.a target/x86_64-apple-ios/release/libozero_api.a \
    -output "$out/sim/libozero_api.a"

xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libozero_api.a -headers "$out/headers" \
    -library "$out/sim/libozero_api.a" -headers "$out/headers" \
    -output "$out/OzeroCore.xcframework"
