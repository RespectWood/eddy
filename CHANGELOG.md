# Changelog

## Unreleased

- **The name is written in lowercase: eddy.** The app is `eddy.app` (executable `eddy`), and every text in the app and the docs says eddy. Bundle ID, permissions and settings are unchanged.
- **Renamed to Eddy** (was openstream): app Eddy.app, bundle ID `com.respectwood.eddy`, link scheme `eddy://` (Tidal redirect `eddy://auth/tidal`), crates `eddy-core` / `eddy-ffi` / `eddy-cli`, CLI `eddy`, repo RespectWood/eddy.
- **New icon:** a lowercase e whose bar runs out as an arrow, amber and ink. App icon, plus the same mark in the menu bar (dimmed while Eddy is disabled). Sources in `assets/brand/`.
- Tidal and Apple Music open the artist's **original release** instead of a "Various Artists" compilation that contains the same recording (ticket 0002). Tidal candidates now combine the ISRC lookup with Tidal search and are ranked by album artist, popularity and release date.
- New setting **Show Artist Page Instead of Song**: Tidal tracks open on the song, highlighted on its album or single (default), or on the artist page.
- `eddy resolve … --explain` prints every candidate and its score.
- **Autoplay (opt-in):** after opening the song, Eddy presses its play button in Tidal (verified) or Apple Music (untested), using the Accessibility permission (ticket 0003).
- Cache entries from earlier versions are discarded.
- **Copy Spotify Link for Current Song** (menu, ⇧⌘C): reads the song playing in Tidal (Accessibility) or Music (AppleScript) and copies its Spotify link, found through Deezer (ISRC), MusicBrainz and Spotify's embed page; no Spotify account needed. Falls back to a Spotify search link. The lookup starts when the menu opens, so the link is usually ready by the click (ticket 0009).
- `eddy share --title … --artist …` CLI command.
- **Copy Link for Current Song** replaces Copy Spotify Link: a submenu copies a link to the song for Tidal, Apple Music or Spotify, and ⇧⌘C copies one for your own service. Links are web links that open on any device; without an exact match, a search link. `eddy share --to spotify|tidal|apple-music --country DE`.
- **Security hardening** (ticket 0010):
  - Every build is signed with the hardened runtime, so no other process can inject code into Eddy and use its Accessibility or Automation permission.
  - The Tidal developer client secret moved from preferences to the Keychain (migrated on first launch). Keychain items stay on this Mac (not restored from backups onto another one).
  - Debug commands need a secret token (`scripts/debug.sh`); the old `debugCommands` setting no longer does anything.
  - A forged `eddy://auth/tidal` redirect no longer cancels a Tidal login in progress, and login errors show Eddy's own text instead of text from the redirect.
  - Short links are always fetched over HTTPS; all network requests are HTTPS only.
  - Clicked links are no longer written to the system log in readable form.
  - IDs and ISRCs from API responses are checked before they go into URLs.
  - CI: read-only token, actions pinned to commits, `cargo audit`, Dependabot.
  - Eddy only accepts the link types and HTML files it's registered for, and checks every link's scheme before opening it.
- TIDAL's logo (linking to tidal.com) in Settings → Accounts, and "TIDAL" written in capitals throughout the app, as TIDAL's Design Guidelines ask.
- Cached matches older than 30 days are now deleted, not just ignored. From the TIDAL profile only your country is kept, never your username, email or name.

## 0.1.0-alpha (unreleased)

First working version.

- Menu bar app for macOS 13+ (no Dock icon) that registers as the web link handler, converts Spotify links and passes every other link straight to your normal browser.
- Spotify track and album links (incl. `intl-xx` prefixes, `?si=` tracking, `spotify.link` / `spoti.fi` short links) open in the native **Apple Music** or **Tidal** app, with the web player only as a fallback when the app isn't installed.
- Apple Music matching via the iTunes Search API. Exact Tidal matching via ISRC/UPC with optional TIDAL API credentials, otherwise a Tidal search.
- Setup window, pause/resume, recent conversions, choice of service and browser, launch at login, notifications for fallbacks.
- Local SQLite cache of conversions (30 days).
- `eddy` CLI: `parse` and `resolve` commands.
