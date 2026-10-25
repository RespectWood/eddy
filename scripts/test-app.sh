#!/usr/bin/env bash
# Runs the Swift app's tests (macos/Tests/EddyTests). Builds the Rust library first.
#
#   scripts/test-app.sh                    # all tests
#   scripts/test-app.sh --filter Route     # extra arguments go to `swift test`
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/build-rust.sh"

cd "$ROOT/macos"
flags=()
developer="$(xcode-select -p)"
if [[ "$developer" == */CommandLineTools ]]; then
    # Command Line Tools without Xcode: Swift Testing is outside the default search
    # paths, and its Foundation overlay module is missing, so overlays are switched off.
    frameworks="$developer/Library/Developer/Frameworks"
    flags=(
        -Xswiftc -F -Xswiftc "$frameworks"
        -Xswiftc -Xfrontend -Xswiftc -disable-cross-import-overlays
        -Xlinker -F -Xlinker "$frameworks"
        -Xlinker -rpath -Xlinker "$frameworks"
    )
fi
# `${flags[@]+…}`: bash 3.2 (macOS) treats an empty array as unset under `set -u`.
swift test ${flags[@]+"${flags[@]}"} "$@"
