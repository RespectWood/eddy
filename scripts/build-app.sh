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
sed -e "s/__VERSION__/${VERSION%%-*}/" -e "s/__BUILD__/$BUILD/" \
    -e "s/__TIDAL_CLIENT_ID__/${EDDY_TIDAL_CLIENT_ID:-}/" \
    Resources/Info.plist > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
cp Resources/AppIcon.icns "$APP/Contents/Resources/"   # from scripts/make-icon.swift

DEV_IDENTITY="openstream Dev"
if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    # The hardened runtime blocks Apple Events (reading Music) without the entitlement.
    codesign --force --options runtime --timestamp --entitlements Resources/eddy.entitlements \
        --sign "$SIGN_IDENTITY" "$APP"
elif security find-certificate -c "$DEV_IDENTITY" >/dev/null 2>&1; then
    codesign --force --sign "$DEV_IDENTITY" "$APP"
else
    # Ad-hoc signature: runs locally; testers must right-click → Open the first time.
    codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

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
