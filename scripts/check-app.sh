#!/usr/bin/env bash
# Checks a build from scripts/build-app.sh before it's shipped: universal binary,
# valid signature with the hardened runtime (also after unzipping), the URL schemes
# eddy needs, and no test-only code.
#
#   scripts/check-app.sh                   # dist/eddy.app and its zip
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/eddy.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ZIP="$ROOT/dist/eddy-$VERSION-macos.zip"
BINARY="$APP/Contents/MacOS/eddy"
PLIST="$APP/Contents/Info.plist"
failures=0

fail() {
    echo "✗ $*" >&2
    failures=$((failures + 1))
}

pass() {
    echo "✓ $*"
}

[[ -d "$APP" ]] || { echo "no $APP: run scripts/build-app.sh first" >&2; exit 1; }

archs="$(lipo -archs "$BINARY")"
if [[ " $archs " == *" arm64 "* && " $archs " == *" x86_64 "* ]]; then
    pass "universal binary ($archs)"
else
    fail "not universal: $archs"
fi

check_signature() {
    local app="$1" label="$2"
    if ! codesign --verify --strict "$app" 2>/dev/null; then
        fail "$label: signature invalid"
        return
    fi
    local signature runtime_flag='flags=0x[0-9a-f]+\(([a-z-]+,)*runtime[,)]'
    signature="$(codesign -dv "$app" 2>&1)"
    if [[ "$signature" =~ $runtime_flag ]]; then
        pass "$label: signed, hardened runtime"
    else
        fail "$label: hardened runtime missing"
    fi
}
check_signature "$APP" "app"

if [[ -f "$ZIP" ]]; then
    unzipped="$(mktemp -d)"
    trap 'rm -rf "$unzipped"' EXIT
    ditto -x -k "$ZIP" "$unzipped"
    check_signature "$unzipped/eddy.app" "app from the zip"
else
    fail "no zip at $ZIP"
fi

# eddy must be offered as a browser (http, https), receive its login redirect (eddy)
# and spotify: links.
schemes="$(plutil -extract CFBundleURLTypes json -o - "$PLIST")"
for scheme in http https spotify eddy; do
    if [[ "$schemes" == *"\"$scheme\""* ]]; then
        pass "URL scheme $scheme"
    else
        fail "URL scheme $scheme not declared"
    fi
done

version="$(plutil -extract CFBundleShortVersionString raw -o - "$PLIST")"
if [[ "$version" == "${VERSION%%-*}" ]]; then
    pass "version $version"
else
    fail "version $version, expected ${VERSION%%-*}"
fi

# Test builds can answer lookups from recorded responses (eddy-core's `test-support`
# feature); a release build must not contain that code.
if strings "$BINARY" | grep -q EDDY_REPLAY; then
    fail "contains test-only code (EDDY_REPLAY)"
else
    pass "no test-only code"
fi

if ((failures > 0)); then
    echo "$failures check(s) failed" >&2
    exit 1
fi
