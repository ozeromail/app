#!/usr/bin/env bash
# Builds the Rust core as OzeroCore.xcframework (device + simulator) with Swift bindings.
set -euo pipefail

cd "$(dirname "$0")/../core"
out="target/ios"
rm -rf "$out"
mkdir -p "$out/headers"

for target in aarch64-apple-ios aarch64-apple-ios-sim; do
    cargo build --release -p ozero-api --target "$target"
done

cargo run -p ozero-bindgen -- generate --language swift \
    --library target/aarch64-apple-ios/release/libozero_api.a --out-dir "$out/swift"

cp "$out/swift/ozero_apiFFI.h" "$out/headers/"
cp "$out/swift/ozero_apiFFI.modulemap" "$out/headers/module.modulemap"

xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libozero_api.a -headers "$out/headers" \
    -library target/aarch64-apple-ios-sim/release/libozero_api.a -headers "$out/headers" \
    -output "$out/OzeroCore.xcframework"
