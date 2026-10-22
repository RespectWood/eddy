# 0002 — Tidal: open the right release and choose the landing page

| | |
|---|---|
| **Status** | Implemented (2026-10-22). Manual tests E8–E10 pending |
| **Phase** | 1 (v0.1 alpha follow-up) |
| **Estimate** | Part A: 1 day · Part B: ½ day |
| **Depends on** | 0001 |

## Problem

Clicking `https://open.spotify.com/track/5rb9QrpfcKFHM1EUbSIurX` (USHER – *Yeah! (feat. Lil Jon & Ludacris)*):

- ✅ Tidal opens (no freeze) and finds the song.
- ❌ It opens a **random "Various Artists" compilation** (*Summer Party Hits*) instead of Usher's own album.
- ℹ️ The song is highlighted in the track list but doesn't play. That's a Tidal limitation and out of scope (see "Out of scope").
- ❌ It doesn't go to the **artist page** either.

## Root causes (investigated 2026-10-22)

### 1. Wrong release: we take the first of many tracks with the same ISRC

The lookup chain works. Deezer returns the correct ISRC `USAR10301423`, but Tidal's `GET /v2/tracks?filter[isrc]=USAR10301423` returns **20 tracks on the first page alone** (plus a `next` cursor). It's the same recording, licensed onto many compilations. `tidal.rs` takes `data[0]`, which happened to be a compilation:

| Tidal track | Album | Album artist | Track popularity |
|---|---|---|---|
| 360985584 ← **picked today** | Summer Party Hits (2024) | Various Artists | 0.34 |
| 287782994 | Beach Party Anthems (2023) | (compilation) | 0.44 |
| 395186370 | Confessions (20th Anniversary Edition) | Usher | 0.16 |
| **11337915** ← **should pick** | **Confessions (Expanded Edition) (2004)** | **Usher** | **0.88** |
| … 16 more compilations | | | |

Useful signals in the API response:
- **Album artist** is `Various Artists` on compilations and the real artist on the original release (`/v2/albums?filter[id]=…&include=artists`).
- **Track `popularity`**: the original release clearly wins (0.88 vs ≤ 0.44).
- **Album `releaseDate`**: the original is the earliest.
- `albumType` does **not** help: every result is `ALBUM`, including compilations.
- Results are **paginated** (20 per page, `links.next` cursor), so the original can be on a later page.

The same problem is likely on **Apple Music**: iTunes results include compilations, marked with `collectionArtistName: "Various Artists"`.

### 2. Playback: out of scope

Tidal's `tidal://` links can only navigate. The handler in TIDAL desktop 2.43.2 has no play command, and the web client's `/track/<id>` route has no autoplay parameter. This ticket accepts that: openstream's job is to **open the right page**. Pressing play stays with the user.

### 3. Artist page: possible, but it can't highlight the song

`tidal://artist/<id>` navigates to the artist page (the `/artist/` route exists). The artist ID comes from the same API response (`include=artists`). But the artist page can't highlight or play a specific song.

## Implementation notes

- **Deezer's ISRC is sometimes the wrong one.** For Rick Astley and Tones And I, Deezer only returned a reissue ISRC that exists in Tidal *only* on compilations, so ISRC lookup alone couldn't find the original. Track candidates are now **every Tidal track with the Deezer ISRC + Tidal's own search results** (`/v2/searchResults?filter[query]=…&include=tracks,tracks.albums,tracks.artists`), filtered by the title, artist and duration matching and ranked together. An exact ISRC match gets a small bonus (+10). This also makes Tidal track lookups work when Deezer finds nothing.
- Tidal's API returns **one** track per ISRC when several ISRCs are requested, but **every** track for a single ISRC. openstream uses the single-ISRC form plus pagination (max 3 pages).
- Ranking weights (`matching::release_score`): album artist = Spotify primary artist +100 · "Various Artists" −100 · popularity × 50 · album title = Deezer album +40/+30/+15 · older release up to +5 · explicit match ±5.
- Cost per uncached Tidal track: 1 Deezer call, 1–3 ISRC pages, 1 search, 1–3 album-artist batches. Cached for 30 days.
- Live check over `tests/fixtures/original_releases.txt` (10 tracks × Apple Music + Tidal): all open the artist's own release.

## Plan

### Part A: pick the original release (must)

In `crates/openstream-core/src/tidal.rs`:
- [x] Request `include=albums&include=artists`. **Repeat the parameter**: a comma-separated `include=albums,artists` is rejected because the comma gets URL-encoded.
- [x] Follow `links.next` for up to **3 pages** (60 candidates).
- [x] Fetch album artists for the candidate albums in one batched call: `/v2/albums?filter[id]=…&include=artists`.
- [x] Rank the candidates with a new `rank_releases()` in `matching.rs`:
  1. Album artist matches the Spotify primary artist: big plus. `Various Artists`: big minus.
  2. Track popularity (main tiebreaker).
  3. Album title similar to the Deezer album title (Deezer already returns it, e.g. *Confessions (Expanded Edition)*): plus.
  4. Earlier `releaseDate`: small plus.
  5. Explicit flag matches Spotify's: small plus.
- [x] Albums (UPC lookups) can return several editions too, so use the same ranking.
- [x] **Apple Music:** in `itunes.rs`, penalize candidates whose `collectionArtistName` is `Various Artists`.
- [x] **Cache:** v0.1 cached wrong picks for 30 days. Add a `schema_version` key to the cache so old entries are ignored.
- [x] **CLI:** `openstream resolve … --explain` prints every candidate with its score, for debugging matches.
- [x] **Tests:** record the Tidal response for `USAR10301423` as a fixture (it contains no secrets) and assert that `11337915` wins. Add a live test behind `--ignored` that runs when `TIDAL_CLIENT_ID` is set.

### Part B: choice of landing page (should)

- [x] New setting **"Open Tidal tracks on: Album (song highlighted) / Artist page"**. Default: Album, since that's the only page that shows the song.
- [x] Artist page: `tidal://artist/<artistId>`, using the primary artist from Part A's response.
- [x] Albums always open the album page.
- [x] Add the setting to the menu (submenu) and the Settings window.

## Acceptance criteria

- [x] The example link opens **Confessions (Expanded Edition)** (track `11337915`) in Tidal, not a compilation.
- [x] 10 popular tracks from the original artists' own albums never land on a "Various Artists" album, in Tidal or Apple Music. Keep the list in `tests/fixtures/` as a live test.
- [x] Old (v0.1) cache entries aren't used.
- [ ] The landing page setting works: Album (highlighted) and Artist page.
- [ ] `docs/manual-tests.md` section E updated.

## Out of scope

- **Automatic playback:** no supported Tidal link format starts playback. Keyboard or Accessibility automation and Tidal's private Connect protocol were considered and dropped.
- Apple Music's landing page (`music://…?i=<id>` also only highlights the song).

## Manual tests to add

- E8: Usher – Yeah! link opens *Confessions*, not a compilation.
- E9: a song that only exists on a compilation (e.g. a soundtrack track) still opens.
- E10: landing page = Artist opens Usher's artist page.
