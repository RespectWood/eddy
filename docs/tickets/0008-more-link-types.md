# 0008 — More link types: artists, playlists, real short links

| | |
|---|---|
| **Status** | Open |
| **Phase** | 2 |
| **Estimate** | Artists ½–1 day · Short links ½ day · Playlists 2–3 days (after a design decision) |
| **Depends on** | 0002, 0004 |

## Problem

Eddy converts **tracks and albums**. Everything else goes to the browser unchanged:
- **Artist** links ("check out this band") open Spotify's web page.
- **Playlist** links are the most-shared Spotify links after tracks, and they can't be converted at all today.
- **Short links** (`spotify.link/…`, `spoti.fi/…`, what Spotify's Share button often copies) are implemented but have **never been tested with a real link**.
- Podcast **episodes/shows** stay out of scope: Tidal has no podcasts.

## Part A: artist links (must)

- **Spotify metadata:** the artist embed page (`open.spotify.com/embed/artist/<id>`) gives the artist's name (verify in the spike, same approach as tracks/albums).
- **Tidal:** `/v2/searchResults?filter[query]=<name>&include=artists`, pick the artist whose name matches exactly (normalized); on ties prefer the one with higher popularity. Open `tidal://artist/<id>`.
- **Apple Music:** iTunes Search `entity=musicArtist`, exact name match → `artistLinkUrl` → `music://music.apple.com/<cc>/artist/<slug>/<id>`.
- **Ambiguous names** (several artists called "Nirvana"): if no single exact match, open the service's **search** for the name instead of guessing.
- Autoplay: none for artists (no single song to play). Optional later: "Play artist" via the artist page's Play button.
- `SpotifyKind::Artist.is_convertible()` → true; cache, tests and fixtures as for tracks.

## Part B: real short links (must, small)

- [ ] Share a track, album and playlist from the **Spotify app** (Share → Copy link) to get real `spotify.link` / `spoti.fi` URLs.
- [ ] Check `expand::expand` against them (redirect vs. branch.io HTML page); fix as needed.
- [ ] Record the response shapes as fixtures (without tracking parameters) and add tests.

## Part C: playlists (design decision needed)

A Spotify playlist doesn't exist on Tidal or Apple Music, so there's no single page to open. Options:

| Option | What the user gets | Effort | Notes |
|---|---|---|---|
| **C1. Picker window** | A small Eddy window with the playlist's title and songs. Clicking a song opens it (+ autoplay); "Open all in Tidal search" fallback | Medium | No extra permissions. Metadata from the Spotify playlist embed page (verify how many tracks it lists; large playlists may be truncated) |
| **C2. Copy into Tidal** | Eddy creates a playlist with the same songs in the user's Tidal library | High | Needs a Tidal **write scope** (`playlists.write`), which new developer apps don't have (see 0004), plus per-track matching. Apple Music needs MusicKit library access (0005) |
| **C3. Play the first song** | Opens and plays the first matched song | Low | Misleading; rejected |

**Recommendation:** C1 now, C2 as a later option once write scopes are available. Decide before Part C starts.

C1 details:
- Spotify playlist embed page → playlist name, owner, track list (title, artists, duration).
- Window lists the tracks; matching each one runs **lazily** (on click, or in the background for the first ~10) to stay within Tidal's usage limits and the cache.
- Window remembers the last few playlists (menu → Recent Conversions).

## Part D: everything else

- `episode`, `show`: stay pass-through (no podcast support on Tidal; Apple Music podcasts live in the separate Podcasts app). Document it in the README.

## Acceptance criteria

- [ ] Artist links open the right artist in Tidal and Apple Music; ambiguous names open a search.
- [ ] Real short links from the Spotify app convert (track, album, artist, playlist).
- [ ] Playlist links behave as decided in Part C (C1: picker window listing the songs, each opening correctly).
- [ ] Unit tests and live tests for each new type; manual tests added (section N).
