# 0009 — Share back: copy a Spotify link for the song you're playing

| | |
|---|---|
| **Status** | Implemented (Tidal verified 2026-10-23; Apple Music untested, no subscription) |
| **Phase** | 4 (reverse direction) |
| **Estimate** | ½-day spike + 1–2 days |
| **Depends on** | 0003 (reading the player), 0002 (matching) |

## Problem

Eddy solves *receiving* Spotify links. The other half: a Tidal or Apple Music user wants to **send** a song to friends who use Spotify. Today they have to search Spotify for it themselves, or send a Tidal link their friends can't open.

## Goal

A menu item **"Copy Spotify Link for Current Song"**. Eddy reads what Tidal (or Music) is playing, finds the same song on Spotify, and copies the link to the clipboard. The user pastes it into Messages.

```
Eddy menu
  Copy Spotify Link for Current Song     ⌘⇧C (optional global shortcut)
  → "Copied: USHER – Yeah! (feat. Lil Jon & Ludacris)"
```

## Design

### 1. What's playing

- **Tidal:** the player bar in the accessibility tree (already used by autoplay to verify): title from `footerPlayer → _trackContent → AXLink`, artists from the artist row. Check in the spike whether the links expose the track URL (`AXURL`), which would give the exact Tidal track ID. Needs the Accessibility permission, like autoplay.
- **Tidal fallback without Accessibility:** the window title shows "<title> - <artists>" while playing (seen in the UI dump), readable without full Accessibility access via the window list. Verify.
- **Apple Music:** Music's AppleScript (`current track`: name, artist, album, duration, and for catalog tracks possibly the store ID). Needs the Automation permission for Music, a separate one-time prompt.

### 2. Find it on Spotify

No Spotify Web API: since March 2026 its Development Mode requires a Spotify Premium account (and allows 5 users, extended quota needs 250k MAU). We don't use it.

| Option | How | Pros | Cons |
|---|---|---|---|
| **S1. MusicBrainz → Spotify** | Tidal track ID → ISRC (Tidal API) → MusicBrainz `GET /ws/2/isrc/<isrc>?inc=url-rels` → Spotify track URLs in the recording's relations | Exact song, free, no key, no account | Coverage: community data. Tested 2026-10-23: *Blinding Lights* 3 links, *Bohemian Rhapsody* 2, *Yeah!* (ISRC USAR10301423) 2, both verified via the embed page. Links may be older/regional track IDs. Rate limit 1 req/s and occasionally "server busy" (retry once), needs a User-Agent with contact info |
| **S1b. Check candidates** | Fetch `open.spotify.com/embed/track/<id>` (existing `spotify.rs` parser) for each candidate; keep the one whose title/artist/duration match (`matching`) | Drops wrong or dead links; picks the best of several | One extra request per candidate (cached) |
| **S2. Spotify search link** | Copy `https://open.spotify.com/search/<artist title>` | Always works | The friend lands on search results, not the song |
| **S3. song.link page** | Copy `https://song.link/<tidal url>` (the web page still works; only the API is gone) | Works for friends on any service | Not a Spotify link; the friend clicks once more. Optional setting |
| ~~Spotify Web API~~ | | | Rejected: needs Spotify Premium |

**Plan:** S1 + S1b, otherwise **S2 as the always-working fallback**. S3 as an optional setting ("Copy as: Spotify link / song.link"). The notification says which one was copied ("Copied Spotify link" vs. "Copied Spotify search link"). Apple Music tracks without an ISRC go straight to S2.

### 3. Output

- Copy the URL to the clipboard; notification with the song.
- Optional: copy as "USHER – Yeah! https://open.spotify.com/track/…" (setting).
- Optional global keyboard shortcut.

## Spike results (2026-10-23)

- [x] **Tidal player bar:** title, every artist, album and length ("4:10") are readable (see `Player.swift`). `AXURL` wasn't needed. The window title ("<title> - <artists>") is the fallback while the web tree isn't built yet.
- [x] **Tidal limitation:** the Accessibility API only sees windows on the current desktop. When Tidal's window is closed or on another Space, there's a clear notification instead of a link.
- [x] **MusicBrainz coverage:** 12 of 20 songs got an exact link (popular hits mostly yes; *HUMBLE.*, *Dancing On My Own*, First Aid Kit, Kent, Håkan Hellström, Little Simz, Molly Nilsson no). Old or regional track IDs work: every candidate is checked on Spotify's embed page.
- [x] **Spotify embed page:** readable without login; title, artists and duration match what Tidal shows.
- [ ] **Music AppleScript:** written (name, artist, album, duration), untested without a subscription.

## Done

- [x] Core: `deezer::find_isrcs` (up to 3 matching ISRCs), `musicbrainz.rs` (ISRC → Spotify track IDs, 1 req/s, User-Agent, one retry on 503), `reverse.rs` (`Resolver::spotify_link`, checks candidates via the embed page and `matching`, falls back to a search link). Exact links cached for 30 days (`spotify_links_v1`).
- [x] FFI: `NowPlaying`, `SpotifyShare`, `Resolver.spotifyLink`.
- [x] CLI: `eddy share --title … --artist … [--duration 4:10] [--explain]`.
- [x] App: `Player.swift` (Tidal via AX, Music via AppleScript, preferred service first), menu item **Copy Spotify Link for Current Song** (⇧⌘C while the menu is open), clipboard, notifications, `eddy://debug/share`.
- [x] Lookup starts when the menu opens, so the link is usually ready at the click. Only the click touches the clipboard; opening the menu never shows a permission prompt (Music is skipped until Automation is allowed). No background polling.
- [x] `NSAppleEventsUsageDescription`; Apple Events entitlement for hardened-runtime (Developer ID) builds.
- [x] Tests: recorded MusicBrainz response, search URLs, share cache; live tests (`--ignored`). Manual tests section O.

## Not done (follow-ups)

- Optional "copy as song.link" setting (S3) and "Artist – Title + link" format.
- Global keyboard shortcut (outside the menu).
- Better coverage for songs MusicBrainz doesn't know (e.g. MusicBrainz recording search by title/artist instead of ISRC).

## Acceptance criteria

- [x] With Tidal playing *Yeah!*, the menu item copies an exact link that opens *Yeah!* on Spotify (S1); a song MusicBrainz doesn't know copies a Spotify search for it (S2).
- [x] No Spotify account, API key or Premium needed.
- [ ] Works for Apple Music where the permission is granted (untested).
- [x] Clear notification of what was copied; nothing copied (and a clear message) when nothing is playing.
- [x] Manual tests added (section O).
