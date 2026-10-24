#!/usr/bin/env bash
# Builds dist/Eddy.app (and a zip of it).
#
#   scripts/build-app.sh                  # universal build, ad-hoc signed
#   ARCHS=arm64 scripts/build-app.sh      # Apple silicon only, faster
#   SIGN_IDENTITY="Developer ID Application: …" NOTARY_PROFILE=eddy scripts/build-app.sh
#       # signed for distribution and notarized (profile created with `xcrun notarytool store-credentials`)
#
# Without SIGN_IDENTITY, a local "openstream Dev" certificate is used when present
# (stable signature, so macOS keeps the Accessibility permission across rebuilds),
# otherwise an ad-hoc signature.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Local, untracked settings (e.g. EDDY_TIDAL_CLIENT_ID) live in .env.
if [[ -f "$ROOT/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$ROOT/.env"
    set +a
fi
ARCHS="${ARCHS:-arm64 x86_64}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
BUILD="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
DIST="$ROOT/dist"
APP="$DIST/Eddy.app"

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
    binaries+=("$(swift build -c release --triple "$arch-apple-macosx13.0" --show-bin-path)/Eddy")
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${binaries[@]}" -output "$APP/Contents/MacOS/Eddy"
strip -x "$APP/Contents/MacOS/Eddy"
# plutil, not sed: the values are written as plain strings, whatever characters they hold.
PLIST="$APP/Contents/Info.plist"
cp Resources/Info.plist "$PLIST"
plutil -replace CFBundleShortVersionString -string "${VERSION%%-*}" "$PLIST"
plutil -replace CFBundleVersion -string "$BUILD" "$PLIST"
plutil -replace EddyTidalClientID -string "${EDDY_TIDAL_CLIENT_ID:-}" "$PLIST"
plutil -lint "$PLIST" >/dev/null
cp Resources/AppIcon.icns "$APP/Contents/Resources/"   # from scripts/make-icon.swift

# Every build gets the hardened runtime: without it, any local process can inject a
# library (DYLD_INSERT_LIBRARIES) and inherit Eddy's Accessibility and Automation
# permissions. The runtime blocks Apple Events (reading Music) without the entitlement.
DEV_IDENTITY="openstream Dev"
sign=(codesign --force --options runtime --entitlements Resources/eddy.entitlements)
if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    "${sign[@]}" --timestamp --sign "$SIGN_IDENTITY" "$APP"
elif security find-certificate -c "$DEV_IDENTITY" >/dev/null 2>&1; then
    "${sign[@]}" --sign "$DEV_IDENTITY" "$APP"
else
    # Ad-hoc signature: runs locally; testers must right-click → Open the first time.
    "${sign[@]}" --sign - "$APP"
fi
codesign --verify --strict "$APP"
# Fail the build rather than ship without the hardened runtime.
# The flags read e.g. `flags=0x10000(runtime)` or, ad-hoc, `flags=0x10002(adhoc,runtime)`.
signature="$(codesign -dv "$APP" 2>&1)"
runtime_flag='flags=0x[0-9a-f]+\(([a-z-]+,)*runtime[,)]'
[[ "$signature" =~ $runtime_flag ]] || { echo "hardened runtime missing" >&2; exit 1; }

ZIP="$DIST/eddy-$VERSION-macos.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

if [[ -n "${NOTARY_PROFILE:-}" ]]; then
    xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
    ditto -c -k --keepParent "$APP" "$ZIP"
fi

echo "Built $APP ($VERSION, $(lipo -archs "$APP/Contents/MacOS/Eddy"))"
echo "      $ZIP"
