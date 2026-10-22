#!/usr/bin/env bash
# Builds dist/openstream.app (and a zip of it).
#
#   scripts/build-app.sh                  # universal build, ad-hoc signed
#   ARCHS=arm64 scripts/build-app.sh      # Apple silicon only, faster
#   SIGN_IDENTITY="Developer ID Application: …" NOTARY_PROFILE=openstream scripts/build-app.sh
#       # signed for distribution and notarized (profile created with `xcrun notarytool store-credentials`)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCHS="${ARCHS:-arm64 x86_64}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
DIST="$ROOT/dist"
APP="$DIST/openstream.app"

rust_targets=""
for arch in $ARCHS; do
    case "$arch" in
        arm64) rust_targets+=" aarch64-apple-darwin" ;;
        x86_64) rust_targets+=" x86_64-apple-darwin" ;;
        *) echo "unknown arch $arch" >&2; exit 1 ;;
    esac
done
TARGETS="${rust_targets# }" "$ROOT/scripts/build-rust.sh"

cd "$ROOT/macos"
binaries=()
for arch in $ARCHS; do
    swift build -c release --triple "$arch-apple-macosx13.0"
    binaries+=("$(swift build -c release --triple "$arch-apple-macosx13.0" --show-bin-path)/Openstream")
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${binaries[@]}" -output "$APP/Contents/MacOS/openstream"
strip -x "$APP/Contents/MacOS/openstream"
sed -e "s/__VERSION__/${VERSION%%-*}/" -e "s/__BUILD__/$BUILD/" Resources/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
else
    # Ad-hoc signature: runs locally; testers must right-click → Open the first time.
    codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

ZIP="$DIST/openstream-$VERSION-macos.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
    ditto -c -k --keepParent "$APP" "$ZIP"
fi

echo "Built $APP ($VERSION, $(lipo -archs "$APP/Contents/MacOS/openstream"))"
echo "      $ZIP"
