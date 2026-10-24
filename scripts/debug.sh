#!/usr/bin/env bash
# Sends a developer command to the running eddy (see macos/Sources/Eddy/DebugCommands.swift).
#
#   scripts/debug.sh share                 # default target; share to=spotify|music|tidal
#   scripts/debug.sh click url=https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC
#   scripts/debug.sh dump app=tidal
#   scripts/debug.sh autoplay app=music kind=track title="Yeah!" artist=USHER url=music://…
#
# The first run creates the secret token that enables debug commands; eddy ignores
# `eddy://debug/…` URLs without it. `scripts/debug.sh off` removes it again.
set -euo pipefail

DOMAIN=com.respectwood.eddy

if [[ $# -lt 1 ]]; then
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
fi

command="$1"
shift

if [[ "$command" == off ]]; then
    defaults delete "$DOMAIN" debugToken 2>/dev/null || true
    echo "Debug commands disabled."
    exit 0
fi

token="$(defaults read "$DOMAIN" debugToken 2>/dev/null || true)"
if [[ ${#token} -lt 16 ]]; then
    token="$(openssl rand -hex 16)"
    defaults write "$DOMAIN" debugToken "$token"
    echo "Debug commands enabled (new token in defaults: $DOMAIN debugToken)."
fi

# Percent-encodes everything except unreserved characters (RFC 3986).
encode() {
    python3 -I -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""), end="")' "$1"
}

query="token=$token"
for arg in "$@"; do
    [[ "$arg" == *=* ]] || { echo "expected key=value, got: $arg" >&2; exit 1; }
    query+="&$(encode "${arg%%=*}")=$(encode "${arg#*=}")"
done

open "eddy://debug/$command?$query"
