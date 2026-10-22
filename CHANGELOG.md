# Changelog

## 0.1.0-alpha (unreleased)

First working version.

- Menu bar app for macOS 13+ (no Dock icon) that registers as the web link handler, converts Spotify links and passes every other link straight to your normal browser.
- Spotify track and album links (incl. `intl-xx` prefixes, `?si=` tracking, `spotify.link` / `spoti.fi` short links) open in the native **Apple Music** or **Tidal** app, with the web player only as a fallback when the app isn't installed.
- Apple Music matching via the iTunes Search API. Exact Tidal matching via ISRC/UPC with optional TIDAL API credentials, otherwise a Tidal search.
- Setup window, pause/resume, recent conversions, choice of service and browser, launch at login, notifications for fallbacks.
- Local SQLite cache of conversions (30 days).
- `openstream` CLI: `parse` and `resolve` commands.
