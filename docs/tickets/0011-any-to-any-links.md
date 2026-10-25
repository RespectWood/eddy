# 0011 — Any to any: convert Spotify, Apple Music and TIDAL links into each other

| | |
|---|---|
| **Status** | Open (later) |
| **Phase** | 4 |
| **Estimate** | ½-day spike + 3–5 days |
| **Depends on** | 0002 (matching), 0004 (Connect TIDAL), 0009 (finding songs on Spotify) |

## Problem

eddy converts in **one direction** only: a clicked **Spotify** link opens in Apple Music or TIDAL. The other cases fall through to the browser:
- A friend sends an **Apple Music** link to a TIDAL user (or the other way round). It opens a web page in a service they don't use.
- A **Spotify user** gets an Apple Music or TIDAL link. eddy has no "Spotify" setting at all today.
- Copy Link (0009) already finds the song playing on all three services, but only for the song you're playing, not for a link you received.

## Goal

Any link from any of the three services opens in **your** service:

| Clicked link ↓ / Your service → | Spotify | Apple Music | TIDAL |
|---|---|---|---|
| **Spotify** | open in Spotify app | ✅ today | ✅ today |
| **Apple Music** | new | open in Music app | new |
| **TIDAL** | new | new | open in TIDAL app |

Same service in and out: no lookup, just open the native app (`spotify:`, `music://`, `tidal://`) instead of the web page. Covers tracks and albums first; artists follow 0008's rules once that's done.

## Design

### 1. Parse the new links (`link.rs`)

Generalize `ParsedLink::Spotify(SpotifyLink)` into a source-agnostic link (`{ service, kind, id }`), keeping the sync, I/O-free `parse`.

- **Apple Music:** `music.apple.com/<cc>/album/<slug>/<id>?i=<trackId>` (track inside an album), `/song/<slug>/<id>`, `/album/<slug>/<id>`, `/artist/<slug>/<id>`; also `itunes.apple.com` and `geo.music.apple.com`. Keep the storefront (`<cc>`) as a hint.
- **TIDAL:** `tidal.com/browse/{track,album,artist}/<id>`, `tidal.com/{track,album,artist}/<id>`, `listen.tidal.com/…`; short links `tidal.link/…` (expand like `spotify.link`, see 0008 Part B).
- Everything else (playlists, videos, music videos, podcasts) stays pass-through for now.

### 2. Read the source item (title, artists, duration, ISRC/UPC)

| Source | How | Notes |
|---|---|---|
| Spotify | embed page (existing `spotify.rs`) | no ISRC; Deezer finds it (existing) |
| Apple Music | iTunes **Lookup** API `itunes.apple.com/lookup?id=<id>&country=<cc>` | no key; no ISRC in the response, so Deezer by artist/title/duration |
| TIDAL | TIDAL API `/v2/tracks/<id>`, `/v2/albums/<id>` (client credentials, existing `tidal.rs` auth) | gives ISRC/UPC directly |

Turn all three into the same metadata struct the matchers already take (today `SpotifyMeta`, to be renamed).

### 3. Find it on the target

- **→ Apple Music / TIDAL:** existing `itunes.rs` / `tidal.rs` matching, fed by the shared metadata.
- **→ Spotify:** existing `reverse.rs` path (ISRC → MusicBrainz → check candidates on the embed page → search link fallback). Albums: MusicBrainz release by UPC/barcode, same check and fallback (verify coverage in the spike).
- Cache per `(source service, kind, id, target)`.

### 4. Settings and app

- **Your service:** add **Spotify** to `Service` (`target.rs`) and to the Settings picker. The README pitch changes from "Spotify links open in Apple Music or TIDAL" to "music links open in your app".
- Default-browser handling: catch `music.apple.com`, `tidal.com`, `listen.tidal.com`, `tidal.link` as well as Spotify. When the clicked link is already your service, open the native app directly.
- Per-source switches (optional): "Convert Apple Music links", "Convert TIDAL links", so someone can keep a service's web links untouched.
- Autoplay (0003) as today for tracks on TIDAL/Apple Music; Spotify target: only open (no autoplay unless a later spike covers the Spotify app).
- CLI: `eddy resolve <any link> --to spotify|apple-music|tidal`.

### TIDAL approval

Turning **TIDAL links into Spotify or Apple Music links** uses TIDAL data to send the user to another service. The TIDAL Developer Guidelines (III) prohibit "Enabling the transfer of data to another service" without TIDAL's express written approval (same concern as 0008 Part C). Describe this direction to TIDAL and get written approval before shipping it, and resubmit if eddy is in Production Mode ("material changes", Developer Terms IV). Fallback if refused: convert TIDAL links via TIDAL's public page metadata only, or leave TIDAL as a target-only service.

## Spike first (½ day)

- [ ] iTunes Lookup API: does it return everything needed (track number, duration, explicitness) for songs, `?i=` album tracks and albums? Which storefront to use when the link's `<cc>` differs from the user's country?
- [ ] TIDAL API: track/album by ID with client credentials only (no user login), and whether ISRC/UPC are included.
- [ ] MusicBrainz coverage for **albums** by UPC → Spotify album link.
- [ ] Real `tidal.link` short links: redirect or HTML page?
- [ ] Spotify desktop app: does `spotify:track:<id>` open and play reliably from eddy?

## Plan

- [ ] Core: generic `ParsedLink`, Apple Music and TIDAL parsers with fixtures, `Service::Spotify`, metadata from iTunes Lookup and TIDAL, resolver routes for all six directions, cache keys.
- [ ] FFI: expose the new link types and `Service::Spotify`.
- [ ] CLI: `parse`/`resolve` accept all three services.
- [ ] App: Settings picker with Spotify, per-source switches, native-app passthrough for same-service links.
- [ ] README (what's converted, data flow table with the new requests), manual tests (new section).

## Acceptance criteria

- [ ] Each of the six cross-service directions opens the right track and album for at least 10 popular and 5 niche songs; misses open a search on the target service.
- [ ] A link to your own service opens the native app with no network requests.
- [ ] Links from services you turned off pass through to the browser unchanged.
- [ ] TIDAL → other services only ships with TIDAL's written approval.
- [ ] Unit tests with recorded responses; live tests (`--ignored`) per direction.
