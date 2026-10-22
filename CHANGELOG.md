# Changelog

## Unreleased

- Tidal and Apple Music open the artist's **original release** instead of a "Various Artists" compilation that contains the same recording (ticket 0002). Tidal candidates now combine the ISRC lookup with Tidal search and are ranked by album artist, popularity and release date.
- New setting: open Tidal tracks on the album (song highlighted) or the artist page.
- `openstream resolve … --explain` prints every candidate and its score.
- **Autoplay (opt-in):** after opening the song, openstream presses its play button in Tidal (verified) or Apple Music (untested), using the Accessibility permission (ticket 0003).
- Cache entries from earlier versions are discarded.

## 0.1.0-alpha (unreleased)

First working version.

- Menu bar app for macOS 13+ (no Dock icon) that registers as the web link handler, converts Spotify links and passes every other link straight to your normal browser.
- Spotify track and album links (incl. `intl-xx` prefixes, `?si=` tracking, `spotify.link` / `spoti.fi` short links) open in the native **Apple Music** or **Tidal** app, with the web player only as a fallback when the app isn't installed.
- Apple Music matching via the iTunes Search API. Exact Tidal matching via ISRC/UPC with optional TIDAL API credentials, otherwise a Tidal search.
- Setup window, pause/resume, recent conversions, choice of service and browser, launch at login, notifications for fallbacks.
- Local SQLite cache of conversions (30 days).
- `openstream` CLI: `parse` and `resolve` commands.
