# 0010 — Security hardening

| | |
|---|---|
| **Status** | Done except manual checks (see Acceptance criteria) |
| **Phase** | 1 (items 1–2 block 0006) |
| **Estimate** | 1–1½ days |
| **Depends on** | — |

## Goal

Fix the problems found in a security audit of `main` (`7d97b1f`, 2026-10-24) before Eddy goes out as a release. Eddy is the default browser, so **every web page can send it URLs**, and with autoplay on it holds the **Accessibility** permission (it can press any button in any app). Most findings come from those two facts.

## Findings

| # | Severity | Finding | Where |
|---|---|---|---|
| 1 | Medium | Builds without `SIGN_IDENTITY` don't use the hardened runtime, so any local process can inject code and inherit Eddy's Accessibility + Automation permissions | `scripts/build-app.sh` |
| 2 | Medium | Tidal developer client **secret** stored in plain text in UserDefaults | `AppModel.swift:16` |
| 3 | Low–Med | Once `debugCommands` is on, **any web page** can run them (simulate clicks, autoplay, dump another app's UI to disk) | `DebugCommands.swift`, `AppModel.handle` |
| 4 | Low | A forged `eddy://auth/tidal` redirect cancels a login in progress and shows attacker-chosen text in Settings | `TidalAccount.handleRedirect` |
| 5 | Low | Keychain item is migratable to other devices and the update path doesn't set accessibility | `Keychain.swift` |
| 6 | Low | `http://` short links are fetched over plain HTTP: a network attacker picks which song opens | `expand.rs`, `link.rs` |
| 7 | Low (privacy) | Every clicked Spotify URL (incl. the `si=` share ID that identifies the sender) is logged with `privacy: .public` | `AppModel.swift:146`, `TargetApps.swift:39`, `Browsers.swift:56` |
| 8 | Low | Values from API responses go into URLs unchecked (ISRC → MusicBrainz path, Tidal IDs → `tidal://` / `tidal.com` URLs) | `musicbrainz.rs:52`, `target.rs:84`, `tidal.rs` |
| 9 | Low | CI has no `permissions:` block, third-party actions are pinned by tag, and dependencies are never checked against RustSec | `.github/workflows/ci.yml` |
| 10 | Info | Client ID is put into Info.plist with `sed` (a `/`, `&` or `<` breaks or injects into the plist) | `scripts/build-app.sh` |

### Details

**1. No hardened runtime.** Only the `SIGN_IDENTITY` branch passes `--options runtime`. The "openstream Dev" and ad-hoc branches don't, and that's what 0006 ships. Without the hardened runtime, `DYLD_INSERT_LIBRARIES=evil.dylib /Applications/Eddy.app/Contents/MacOS/Eddy` loads attacker code into Eddy, and that code inherits Eddy's TCC grants: Accessibility (synthetic clicks and key presses in every app) and Apple Events to Music. This is a well-known way to get around TCC.

**2. Client secret in UserDefaults.** `~/Library/Preferences/com.respectwood.eddy.plist` can be read by every process running as the user without a prompt, and it ends up in Time Machine and migration backups. The user's own login tokens already go to the Keychain. The developer secret should too.

**3. Debug commands from the web.** The doc comment says "so a web page can't trigger them", but that only holds while the flag is off. Once a developer turns it on (and it stays on), any page can open `eddy://debug/autoplay?app=music&...` or `eddy://debug/click?url=...`, and the browser's "Open Eddy?" prompt is the only barrier. `/autoplay` presses buttons through the Accessibility API, and `/dump` writes Tidal's or Music's whole UI tree to disk.

**4. OAuth redirect.** PKCE and `state` are correct (the core rejects a forged code). But `handleRedirect` clears `pending` **before** checking anything, and `code_from_callback` reads `error` **before** checking `state`. So a page that fires `eddy://auth/tidal?error=x&error_description=<any text>` during a login cancels that login and shows `Tidal login failed: <any text>` in Settings. That text could be used for phishing ("call support at…").

**5. Keychain.** `kSecAttrAccessibleAfterFirstUnlock` can be restored onto another Mac from a backup. Tokens for one device should use `…ThisDeviceOnly`. `SecItemUpdate` also leaves the old attribute as it was.

**6. Plain-HTTP short links.** `parse` accepts `http://spotify.link/…`, and `expand` fetches it as written. Someone on the same Wi-Fi can answer with any `open.spotify.com` redirect, which only lets them pick the song, but it's free to fix.

**7. Logging.** Unified logs end up in sysdiagnose and support bundles and can be read with `log show`. `si=` shows who shared the link with whom.

**8. Untrusted values in URLs.** These come from TLS-protected APIs, so the risk is low. But a bad value (an ISRC like `../…`, a Tidal ID like `1?x=…`) changes the URL that's requested or opened. Checking the format costs very little.

**9. CI.** It runs on `pull_request` with the default token permissions, and `Swatinem/rust-cache@v2` / `actions/*@v4` are mutable tags. 0006 adds a release job with write access, so least privilege should be in place first. `cargo-audit` isn't installed locally either, so the dependencies (ureq 2.12.1, rustls 0.23.46, rusqlite 0.32.1, uniffi 0.28.3, …) haven't been checked against RustSec in this audit.

### Checked, no issue found

- PKCE S256 with `getrandom`; `state` compared exactly; tokens only in the Keychain; `TidalAuth`'s `Debug` hides secrets.
- Link parsing: strict host allowlist, 22-character base62 ID check, sync and no I/O. Non-Spotify links are forwarded without any network call.
- Converted items open only in the target app (`withApplicationAt:`). Native URLs use custom schemes, so they can't loop back to Eddy.
- SQLite: values are bound parameters; table names are constants.
- The Music AppleScript is a fixed string with no interpolation.
- Tidal pagination `next` is appended as a path to a fixed host, so it can't redirect the bearer token elsewhere. ureq doesn't forward `Authorization` across redirects.
- `.env` and `*.p8` are git-ignored; the Tidal client ID in Info.plist is public by design (PKCE).

## Scope

### 1. Hardened runtime on every build (blocks 0006)
- [x] `build-app.sh`: sign with `--options runtime --entitlements Resources/eddy.entitlements` in **all three** branches (ad-hoc and dev certificate included; `--timestamp` only with `SIGN_IDENTITY`).
- [x] Don't add `disable-library-validation` or `allow-dyld-environment-variables`.
- [x] Check: `codesign -dv dist/Eddy.app` shows `flags=0x10000(runtime)`; `DYLD_INSERT_LIBRARIES=/tmp/x.dylib dist/Eddy.app/Contents/MacOS/Eddy` doesn't load it; autoplay and "Copy Spotify Link" (Music) still work.

### 2. Client secret to the Keychain (blocks 0006)
- [x] Store `tidalClientSecret` with `Keychain` (service `com.respectwood.eddy.tidal-dev`), not UserDefaults.
- [x] Migration on launch: if the old defaults key exists, move it to the Keychain and `removeObject(forKey:)`.
- [x] Client ID can stay in defaults (not secret).

### 3. Lock down debug commands
- [x] Replace the `debugCommands` bool with a random token: `defaults write com.respectwood.eddy debugToken <random>`. Only accept `eddy://debug/…?token=<that>`. A web page can't read the user's defaults.
- [x] Compare the tokens in constant time; ignore commands without a token and don't log them.
- [x] Update the doc comment, the `log` gate and `docs/manual-tests.md`. A small `scripts/debug.sh <command> [args]` that reads the token and calls `open` keeps it convenient.

### 4. Validate the OAuth redirect before acting on it
- [x] Core: in `code_from_callback`, check `state` **first**; return `StateMismatch` for an `error` with the wrong or no `state`.
- [x] FFI: export a cheap `tidal_callback_matches(request, url) -> bool` (no network).
- [x] `TidalAccount.handleRedirect`: if it doesn't match, ignore it and **keep** `pending`; clear `pending` only for a matching redirect.
- [x] Don't show `error_description` as is: map the known OAuth `error` codes (`access_denied`, …) to our own text; anything else becomes "Tidal login failed. Try again."
- [x] Unit tests: forged `error` without state, forged `error` with wrong state, real error with right state.

### 5. Keychain attributes
- [x] `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` on add **and** update (so existing items get migrated).

### 6. HTTPS only for short links

Also: `https_only` on every request (all APIs are HTTPS).

- [x] `expand`: rewrite `http://` to `https://` before fetching.
- [x] Only follow redirects to `https`, at most 5 (handle redirects by hand for the short-link request, or check `response.get_url()` scheme and host for each hop).
- [x] Test: `http://spotify.link/x` is requested as `https://`.

### 7. Logging privacy
- [x] Clicked URLs: log `kind/id` with `.public` and the full URL with `.private` (or not at all). Same in `TargetApps.open` and `Browsers.forward`.
  Done as: only the host is public; URLs and network errors (they include the request URL) are private.
- [x] `grep -rn "privacy: .public" macos/` and check every hit carries no URL, title or token.

### 8. Validate values from APIs
- [x] `musicbrainz::spotify_track_ids`: accept only ISRCs that match `^[A-Z]{2}[A-Z0-9]{3}[0-9]{7}$`; otherwise return empty.
- [x] `target::tidal_urls`: Tidal IDs are digits only, otherwise no match (return `None` and fall back to search).
  Changed after reading the TIDAL API docs ("resource IDs are opaque strings"): letters, digits, `-` and `_`, at most 64 characters, so a future ID format still works while `/ ? # @ % .` stay out.
- [x] Deezer item IDs: typed as integers (check the struct). Already `u64`.
- [x] `apple_music_urls`: host must be `music.apple.com` or end with `.music.apple.com` (today: any host ending in `music.apple.com`).

### 9. CI and dependencies
- [x] `permissions: contents: read` at the top of `ci.yml`.
- [x] Pin `actions/checkout`, `actions/upload-artifact`, `Swatinem/rust-cache` to commit SHAs (comment with the tag).
- [x] Add a `cargo audit` step (or `cargo deny check advisories`); run it once locally and fix or document what it finds.
  Result (2026-10-24): no vulnerabilities. Three "unmaintained" notices, all from uniffi 0.28 at build or binding-generation time: `basic-toml` (RUSTSEC-2026-0374), `bincode` (RUSTSEC-2025-0141), `paste` (RUSTSEC-2024-0436). They go away with a uniffi upgrade (separate work).
- [x] Dependabot for `cargo` and `github-actions`.

### 10. Build hygiene
- [x] Set the version, build and client ID with `plutil -replace` instead of `sed`.

### 11. Follow-up from the re-audit (2026-10-24)
- [x] `build-app.sh`: the hardened-runtime check also accepts ad-hoc signatures (`flags=0x10002(adhoc,runtime)`); the old check broke the CI app job.
- [x] Only accept the URL schemes Eddy is registered for (`http`, `https`, `spotify`, `eddy`). Any app can send others with `open -a Eddy <url>`, and Eddy would pass them on to the browser with its own rights.
- [x] Only accept HTML/XHTML files (the registered document types); apps, scripts and other files are refused instead of being handed to the browser.
- [x] Check the scheme of every URL before opening it: `music:`/`tidal:` for the app, `https` for the web player, `http`/`https`/`spotify` for "Open Original Link" (the cache and Recents are read back from disk).
- [x] Debug log: line breaks in messages are escaped, so a song title can't fake log entries.
- [x] Network error notifications no longer include the request URL (it holds the song search); details only go to the private log.
- [x] Dependabot: no semver-major updates for Rust crates (they need code changes, e.g. ureq 3).
- [x] CLI `--explain`: control characters in titles and names from APIs are replaced, so they can't send escape sequences to the terminal.

## Out of scope

- Developer ID signing + notarization (PLAN.md Phase 3). Item 1 is the part that doesn't need it.
- Encrypting the conversion cache (`cache.sqlite` holds 30 days of clicked songs). It's in the user's own Application Support; we can look at it again if sync or sharing ever comes.
- Threats from a local attacker who already runs code as the user, beyond item 1 (they can already read the browser profile, etc.).

## Acceptance criteria

- [x] Every release build has the hardened runtime; a dylib injected with `DYLD_INSERT_LIBRARIES` doesn't load. (Checked: an ad-hoc build without the runtime loads a test dylib, the new build doesn't. `build-app.sh` fails if the flag is missing.)
- [ ] No secret in `defaults read com.respectwood.eddy`; an existing secret is migrated.
- [ ] `open "eddy://debug/share"` (no token) does nothing, even with debug commands set up.
- [ ] A forged `eddy://auth/tidal?error=…` during a login is ignored and the login still finishes.
- [x] `cargo test` covers the redirect, ISRC/ID and short-link checks. Short links checked live over `https` and `http` with the CLI.
- [ ] CI green incl. the audit step (after pushing).
- [ ] Manual tests L1–L7 (Connect Tidal), K (autoplay) and the "Copy Spotify Link" tests pass on the hardened build.
