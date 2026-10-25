# 0012 — Automated tests: offline resolver tests, Swift tests, scheduled live tests

| | |
|---|---|
| **Status** | Done except the first scheduled live run (see Acceptance criteria) |
| **Phase** | 2 (before 0011) |
| **Estimate** | 5–7 days, in parts that ship on their own (see Plan) |
| **Depends on** | — |

## Problem

What's tested today (`v0.1.0-beta.1`, 2026-10-25):

| Layer | Automated tests | Gap |
|---|---|---|
| `eddy-core` parsing, matching, cache, TIDAL login, security checks | 51 unit tests + link fixture corpus, run in CI | Good |
| `eddy-core` network code (resolver, iTunes, Deezer, MusicBrainz, TIDAL, Spotify embed, short links) | Parsers tested on a few recorded responses; the full flow only in 10 `#[ignore]` live tests | Live tests never run in CI and nobody knows when they last passed. `itunes.rs` has no offline test at all. Errors (HTTP 429, broken JSON, timeouts) are untested |
| `eddy-ffi` (442 lines) | None | Type and error mapping to Swift untested |
| `eddy-cli` | None | |
| Swift app (~2,200 lines) | None | Link routing, forwarding to the browser, now-playing parsing, autoplay matching: all only tested by hand |
| Release build | `build-app.sh` fails without the hardened runtime | Universal binary and Info.plist not checked |

The manual checklist ([manual-tests.md](../manual-tests.md)) is long (~80 rows), and for beta.1 only one row has a recorded result. Much of it checks logic that a test could cover. The parts that really need eyes (system dialogs, real apps, notifications) get lost among the rest.

Why the resolver has no offline tests: `Http` hands out `ureq::Request`s that call sites send themselves (about 15 places in 8 files), and the agent is `https_only`. Without a seam, nothing can answer those requests in a test, and a local mock server over plain HTTP is refused.

## Goal

- Every conversion path runs **offline in `cargo test`** against recorded responses, including the error cases.
- The Swift app has a test target in CI for its logic: routing, forwarding, parsing of player text.
- Live tests run **on a schedule** in CI, so a changed API is noticed before testers notice it.
- The manual checklist shrinks to what really needs a person, and each remaining row says why.

## Design

### 1. HTTP seam in `eddy-core` (`http.rs`)

Call sites stop touching `ureq` directly. `Http` gets one entry point:

```rust
pub(crate) struct Request { method, url, query: Vec<(String, String)>, headers, form: Option<…>, follow_redirects: bool }
pub(crate) struct Response { status: u16, headers, body: String }

impl Http {
    pub fn send(&self, request: Request) -> Result<Response, ResolveError>;
}
```

with small helpers (`get(url).query(..)`, `json::<T>(…)`, `text(…)`) so call sites stay short. `Http` has two backends:

- **Live:** today's `ureq` agents, unchanged (`https_only`, timeouts, redirect handling).
- **Recorded** (`#[cfg(test)]` or a `test-support` feature): answers from a table of `(method, URL with sorted query) → (status, headers, fixture file)`. It **logs every request**, so tests can assert "no network request" and "exactly one request". A request with no recording **fails the test and prints the URL**, so it's clear what to record next.

`ResolverConfig` gets a way to pass the `Http` in (crate-private or behind the feature), so `Resolver` tests use the recorded backend. No new crate needed.

### 2. Recorded responses (fixtures)

- Directory per service: `tests/fixtures/{itunes,deezer,musicbrainz,tidal,spotify}/…`. The existing files move there.
- A recorder: running the live tests with `EDDY_RECORD=1` saves each response as a fixture. Tokens, `Authorization` headers and anything account-specific are **removed** before saving (fixtures go into a public repo). A test checks that no fixture contains `access_token`, `Bearer ` or `client_secret`.
- Each fixture records where it came from and when (a comment line, or a `_meta` key in JSON).

### 3. Offline resolver tests (`eddy-core`)

Through `Resolver`, from link to `Conversion`, with recorded responses:

- **Spotify → Apple Music:** track, album, "– Remastered 2011" title, `intl-de` link, explicit vs. clean version.
- **Spotify → TIDAL:** with credentials (exact track, album, artist landing page); without credentials (search link, no TIDAL request); with a user login.
- **Right release:** the 10 songs from `original_releases.txt` resolve to the original album, offline (today only live).
- **Not found:** iTunes/TIDAL return nothing → `NotFound` with search URLs.
- **Errors:** HTTP 429 → `RateLimited`; 500 → `Network`; broken JSON; empty body; timeout (the recorded backend can return a transport error). The app's fallback depends on these, so each one is a test.
- **Short links:** `spotify.link` redirect chain, a redirect to a non-Spotify page ends without a conversion, `http://` is requested as `https://`.
- **Cache:** the second resolve makes **zero** requests; a remembered miss makes zero requests for 10 minutes and one after.
- **Share links (`reverse.rs`):** TIDAL → Spotify, Apple Music → Spotify and → TIDAL, with ISRC, without ISRC (search fallback), MusicBrainz with several candidates.
- **TIDAL login (`tidal_auth.rs`):** token exchange and refresh with recorded responses, expired refresh token → the error the app turns into "Reconnect TIDAL".
- **`itunes.rs`:** own unit tests on recorded search responses (none today).

### 4. Parser robustness

- `proptest` (dev-dependency): `parse()` never panics on any string, and `is_music_link(u) == !matches!(parse(u), NotMusic)` for any string.
- Add the link shapes that real tester reports bring to `links.txt` (one line each).

### 5. `eddy-ffi` and `eddy-cli`

- **FFI:** Rust-side tests that every `ResolveError` variant and `Conversion` field survives the conversion to the FFI types. The Swift tests (below) call the generated bindings, which covers the rest.
- **CLI:** integration tests that run the built binary (`env!("CARGO_BIN_EXE_eddy")`): `parse` output for a few links, exit codes for bad input, `resolve` against the recorded backend (via an env var that only test builds read) or left to the live tests.

### 6. Swift test target (`macos/`)

Add `EddyTests` to `Package.swift` (`.testTarget`, `@testable import Eddy`; SwiftPM supports this for executable targets on macOS). CI runs `scripts/build-rust.sh` and then `swift test` in `macos/`.

Small refactors so the logic can be tested without `NSWorkspace`:

- **Routing:** pull the decision out of `AppModel.handle` into a pure function, e.g. `Route.decide(url, isDisabled:, service:) -> .tidalAuth | .debug | .forward | .passThrough(canonical:) | .convert`. `handle` just switches on it. Tests:
  - ordinary links (`https://example.com`, file URLs, lookalike hosts) **always** `.forward`. That is the "most important" manual section C, as a test;
  - disabled / no service → pass-through; playlists, episodes, artists → pass-through (manual F1–F3);
  - `eddy://auth/tidal` vs. `eddy://debug/…` vs. other `eddy://` URLs.
- **Forwarding:** pull the browser choice out of `Browsers.forward` (`candidates(preferred:, own:)`): never eddy itself, falls back to Safari.
- **Landing page:** `landingURLs(for:service:)` with the TIDAL artist option.
- **Pure helpers already there:** `Player.songFromWindowTitle`, `splitArtists`, `milliseconds(fromClock:)`, `AutoPlay.same`/`normalize`, Music's `catalogIDs`, `RecentConversion` Codable round trip (old saved recents still decode).
- **TIDAL redirect (0010 finding 4):** a forged `error` callback during a login leaves the login pending. Needs `TidalAccount` to take its Keychain/opener as parameters, or this stays a manual check.
- **Autoplay against recorded UI trees (optional, later):** `eddy://debug/dump` already saves TIDAL's and Music's accessibility tree. With a small node protocol over `AXUIElement`, `playControl(in:for:)` could run against saved dumps. Worth doing once a TIDAL update breaks autoplay; skip until then.

### 7. CI

- `ci.yml`, `rust` job: unchanged (`fmt`, `clippy`, `cargo test`); the new tests just run.
- `ci.yml`, `app` job: `swift test` before `build-app.sh`; after it, check the build: `lipo -archs` shows `x86_64 arm64`, `codesign --verify --strict`, the hardened runtime flag, and Info.plist declares the `http`/`https`/`eddy` URL schemes.
- New `live.yml`: **weekly** (and by hand via `workflow_dispatch`), `cargo test -- --ignored`. TIDAL client ID/secret come from repository secrets; the workflow runs only on `main`, never for pull requests (secrets in a public repo). A failure means an API changed: re-record the fixtures, fix, or open a ticket. Weekly keeps us well inside TIDAL's per-app rate limit, which all users share (0006).
- Optional: `cargo llvm-cov` report as a CI artifact, no threshold.

### 8. Manual checklist

Once the tests above exist, mark the rows they cover as "automated" in [manual-tests.md](../manual-tests.md) (C, F1–F3, the routing part of H1, I4's fallback, D6/E caching) and keep only what needs a person: system dialogs (B), real apps opening and playing (D, E, K), login in a browser (L), launch at login (I1–I2), link sources (G), notifications.

## Plan

Each step can land on its own.

- [x] **A. Seam (1–1½ days):** `Http::send` with live and recorded backends; move all call sites; existing tests still pass, no behaviour change (the live tests still pass too).
- [x] **B. Fixtures and offline resolver tests (1½–2 days):** recorder with secret stripping, fixtures per service, the tests from Design §3, `itunes.rs` unit tests.
- [x] **C. Robustness, FFI, CLI (½ day):** proptest, FFI conversion tests, CLI integration tests.
- [x] **D. Swift tests (1½ days):** test target, `Route.decide` and `Browsers.candidates` refactors, tests from Design §6, `swift test` in CI.
- [x] **E. CI (½ day):** build checks in the `app` job, `live.yml` with secrets.
- [x] **F. Manual checklist (¼ day):** mark automated rows, shorten the rest.

0011 builds on A and B: its new parsers and lookups (iTunes Lookup, TIDAL `/v2/tracks`) get recorded responses from the start.

## Out of scope

- UI tests that click through menus and Settings (XCUITest needs an Xcode project; SwiftPM can't host them).
- Tests that drive the real TIDAL or Music app in CI (no logged-in apps on CI runners).
- A coverage threshold.

## Acceptance criteria

- [x] `cargo test` resolves every conversion and share direction eddy supports today **without network access** (checked 2026-10-25: `cargo test` and `scripts/test-app.sh` pass under `sandbox-exec` with all network access denied). TIDAL albums are the exception, see Notes.
- [x] Every `ResolveError` variant comes out of at least one test.
- [x] No fixture contains a token or secret (enforced by a test).
- [x] `swift test` runs in CI. Ordinary links always forward to the browser, never to eddy itself (tested).
- [x] The `app` job fails if the build isn't universal, isn't signed with the hardened runtime, or is missing a URL scheme.
- [ ] `live.yml` has run green at least once on `main`. Needs the `TIDAL_CLIENT_ID` and `TIDAL_CLIENT_SECRET` repository secrets; without them the TIDAL live tests skip themselves (the workflow warns).
- [x] `manual-tests.md` marks which rows are automated.

## Notes (2026-10-25)

What was built, where it differs from the design above, and what it found:

- **Seam:** `Http::send(&Request)` with a live and a replay backend (`http.rs`, `replay.rs`). Any HTTP status is a response; `fetch`/`json` turn error statuses into errors. Replay matches `METHOD url` with the query sorted; a recording without a query matches any query; `Replay::first` puts an error in front of a recorded success.
- **`test-support` feature** (eddy-core): lets the CLI's tests answer from a cassette via `EDDY_REPLAY`. Only the CLI's dev-dependency turns it on; a release build with it fails to compile, and `scripts/check-app.sh` checks the shipped binary for it.
- **Cassettes** (`tests/fixtures/cassettes/`, see the README there): recorded 2026-10-25 by the live tests with `EDDY_RECORD=1`, about 1 MB. All 13 live tests passed that day (the TIDAL ones skipped: no developer secret on the dev Mac).
- **TIDAL offline tests** use `tidal_usher_resolve.json`, assembled from the TIDAL responses recorded 2026-10-22 plus hand-written token and search responses. `live_tidal_usher` re-records it once someone runs it with a TIDAL secret. **Not covered offline:** TIDAL albums (UPC lookup) and the ten `original_releases.txt` songs on TIDAL (Apple Music is covered); both need a recording with a TIDAL secret.
- **Swift tests** use Swift Testing (the dev Mac has Command Line Tools, no Xcode, so no XCTest). `scripts/test-app.sh` adds the framework paths that Command Line Tools need and turns off cross-import overlays (its `_Testing_Foundation` module is empty). Refactors: `Route.decide`, `Browsers.candidates`, a static `AppModel.landingURLs`, and `TidalAccount`/`KeychainTidalTokenStore` take their client ID, Keychain item and login-page opener as parameters (tests use their own Keychain item).
- **Not done:** the autoplay tests against saved accessibility trees (optional, see §6) and the coverage report (optional, §7).
- **Findings:**
  - The iTunes Search API (US, 2026-10-25) returns only the *clean* versions of Kendrick Lamar – HUMBLE., so an explicit Spotify track opens its clean version on Apple Music. `apple_music_explicit_track_without_an_explicit_result` pins today's behaviour. A fix would need the iTunes Lookup API (`/lookup?id=<album>&entity=song` does list the explicit tracks) and is a ticket of its own.
  - For "Copy Link for Current Song → Spotify", USHER – Yeah! gives `open.spotify.com/track/1M18Qdzl9xFVRmUm66zn34`, not the `5rb9…` link people usually share: MusicBrainz links both, and the other release scores higher. Both play the same recording.
