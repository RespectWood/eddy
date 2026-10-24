# Changelog

## 0.1.0-beta.1 (not yet released)

First public test release. (A `0.1.0-alpha` was planned but never released; everything below is new.)

### What eddy does

- **Spotify links open in Apple Music or TIDAL.** eddy registers as the macOS default web browser, converts clicked Spotify track and album links (including `intl-xx` links, `?si=` tracking, `spotify.link` / `spoti.fi` short links and `spotify:` links) and opens them in the Music or TIDAL app; the web player only when the app isn't installed. Every other link goes straight to your normal browser.
- **Matching:** Apple Music through the iTunes Search API; TIDAL exactly by ISRC/UPC (via Deezer) plus TIDAL search, preferring the artist's **original release** over "Various Artists" compilations. Without an exact match, the service's search opens.
- **Connect TIDAL:** log in with your own TIDAL account (OAuth with PKCE, in your browser) for exact matches. The login stays in the Keychain on your Mac.
- **Copy Link for Current Song** (menu, ⇧⌘C): copies a link to the song playing in TIDAL or Music, for TIDAL, Apple Music or Spotify (submenu); ⇧⌘C copies one for your own service. Web links that open on any device; a search link when there's no exact match.
- **Autoplay** (optional): presses Play once the song has opened, using the Accessibility permission. Verified with TIDAL; untested with Apple Music.
- **Show Artist Page Instead of Song** (TIDAL): open the artist instead of the song.
- Menu bar app for macOS 13+ (no Dock icon): setup window, Disable/Enable, Recently Opened, choice of service and browser, launch at login, notifications when a link couldn't be matched exactly.
- Matches are cached locally for 30 days, then deleted.
- Icon: a lowercase e whose bar runs out as an arrow (`assets/brand/`). The name is written in lowercase: **eddy**.

### Privacy and security

- No server, no account of its own, no analytics; nothing is sent to the developer. What goes where: [README → Privacy](README.md#privacy).
- From the TIDAL profile only your country is kept, never your username, email or name.
- Hardened runtime on every build, secrets only in the Keychain (this Mac only), HTTPS only, debug commands only with a secret token, forged TIDAL login redirects ignored, only the link types eddy is registered for are accepted, and values from APIs are checked before they go into links (ticket 0010).
- TIDAL branding as TIDAL's guidelines ask: TIDAL's logo linking to tidal.com in Settings → Accounts, "TIDAL" in capitals.

### Known limitations

- **Not notarized by Apple.** The first launch needs System Settings → Privacy & Security → **Open Anyway**.
- **TIDAL is in development mode:** one shared usage limit for all users, so busy moments can fall back to TIDAL search ("TIDAL is rate limiting requests"). TIDAL may not yet let every account sign in with Connect TIDAL; without it, TIDAL opens a search instead of the exact song.
- Apple Music autoplay and Apple Music in general are less tested than TIDAL.
- Spotify links clicked *inside* your browser stay in the browser; eddy only sees links from other apps (ticket 0007).
- Artist, playlist and podcast links aren't converted; they open as usual (ticket 0008).

### For developers

- `eddy` CLI: `parse`, `resolve … --to tidal|apple-music [--explain]`, `share --title … --artist … --to spotify|tidal|apple-music`.
- `scripts/debug.sh` sends developer commands to the running app (creates its token on first use).
