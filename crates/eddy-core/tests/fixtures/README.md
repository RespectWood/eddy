# Test fixtures

- `links.txt`: link shapes and what the parser should return (`tests/links.rs`).
- `original_releases.txt`: songs that are also on many compilations; each must open on the artist's own release.
- `*.json`, `*.html` at this level: single recorded responses, read by unit tests.
- `cassettes/`: whole lookups, recorded, for the offline tests (`src/offline_tests.rs`, the CLI tests).

## Cassettes

A cassette holds every request of one lookup (method and URL, query sorted) with its response. Offline tests answer requests from it; a request that isn't in the cassette fails the test and names the missing URL.

Recording: the live tests write the cassette named in their `live("…")` call.

```sh
EDDY_RECORD=1 cargo test -p eddy-core -- --ignored --test-threads=1
# TIDAL cassettes need TIDAL_CLIENT_ID and TIDAL_CLIENT_SECRET as well.
```

Then run `cargo test`: the offline tests now check the new responses, and their expected IDs may need updating if a service changed its catalog.

What's saved, and what isn't:

- Only the request line and the response. Request headers and form bodies (credentials) are never written.
- Tokens in responses (`access_token`, `refresh_token`, the Spotify embed page's `accessToken`, …) become `REDACTED`. `fixtures_contain_no_secrets` checks every file here.
- Spotify embed pages are cut down to their `__NEXT_DATA__` script, the only part eddy reads.

`tidal_usher_resolve.json` is assembled rather than recorded (its `note` says from what), because recording needs a TIDAL developer app. `live_tidal_usher` replaces it with a real recording.
