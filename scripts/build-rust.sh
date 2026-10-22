#!/usr/bin/env bash
# Builds the Rust FFI library and generates the Swift bindings used by the macOS app.
#
#   scripts/build-rust.sh              # universal (arm64 + x86_64) release build
#   TARGETS=aarch64-apple-darwin scripts/build-rust.sh   # one architecture, faster
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGETS="${TARGETS:-aarch64-apple-darwin x86_64-apple-darwin}"
GENERATED="$ROOT/macos/Generated"
SWIFT_OUT="$ROOT/macos/Sources/Openstream/Generated"

export MACOSX_DEPLOYMENT_TARGET=13.0
cd "$ROOT"

libs=()
for target in $TARGETS; do
    cargo build --release -p openstream-ffi --lib --target "$target"
    libs+=("target/$target/release/libopenstream_ffi.a")
done

mkdir -p "$GENERATED/lib" "$SWIFT_OUT"
# Only the static library goes into the link directory, so the app never depends on a dylib.
lipo -create "${libs[@]}" -output "$GENERATED/lib/libopenstream_ffi.a"

first_target="${TARGETS%% *}"
cargo run --quiet -p openstream-ffi --bin uniffi-bindgen -- generate \
    --library "target/$first_target/release/libopenstream_ffi.dylib" \
    --language swift --out-dir "$GENERATED/bindings"

cp "$GENERATED/bindings/openstream_ffiFFI.h" "$GENERATED/"
cp "$GENERATED/bindings/openstream_ffi.swift" "$SWIFT_OUT/"
echo "Rust library and Swift bindings ready in $GENERATED"
