# Changelog

## Unreleased

- **Renamed to Eddy** (was openstream): app Eddy.app, bundle ID `com.respectwood.eddy`, link scheme `eddy://` (Tidal redirect `eddy://auth/tidal`), crates `eddy-core` / `eddy-ffi` / `eddy-cli`, CLI `eddy`, repo RespectWood/eddy.
- **New icon:** a lowercase e whose bar runs out as an arrow, amber and ink. App icon, plus the same mark in the menu bar (dimmed while Eddy is disabled). Sources in `assets/brand/`.
- Tidal and Apple Music open the artist's **original release** instead of a "Various Artists" compilation that contains the same recording (ticket 0002). Tidal candidates now combine the ISRC lookup with Tidal search and are ranked by album artist, popularity and release date.
- New setting **Show Artist Page Instead of Song**: Tidal tracks open on the song, highlighted on its album or single (default), or on the artist page.
- `eddy resolve … --explain` prints every candidate and its score.
- **Autoplay (opt-in):** after opening the song, Eddy presses its play button in Tidal (verified) or Apple Music (untested), using the Accessibility permission (ticket 0003).
- Cache entries from earlier versions are discarded.
- **Copy Spotify Link for Current Song** (menu, ⇧⌘C): reads the song playing in Tidal (Accessibility) or Music (AppleScript) and copies its Spotify link, found through Deezer (ISRC), MusicBrainz and Spotify's embed page; no Spotify account needed. Falls back to a Spotify search link. The lookup starts when the menu opens, so the link is usually ready by the click (ticket 0009).
- `eddy share --title … --artist …` CLI command.

## 0.1.0-alpha (unreleased)

First working version.

- Menu bar app for macOS 13+ (no Dock icon) that registers as the web link handler, converts Spotify links and passes every other link straight to your normal browser.
- Spotify track and album links (incl. `intl-xx` prefixes, `?si=` tracking, `spotify.link` / `spoti.fi` short links) open in the native **Apple Music** or **Tidal** app, with the web player only as a fallback when the app isn't installed.
- Apple Music matching via the iTunes Search API. Exact Tidal matching via ISRC/UPC with optional TIDAL API credentials, otherwise a Tidal search.
- Setup window, pause/resume, recent conversions, choice of service and browser, launch at login, notifications for fallbacks.
- Local SQLite cache of conversions (30 days).
- `eddy` CLI: `parse` and `resolve` commands.
