# TIDAL Production Mode submission

Text for the Production Mode request in the TIDAL developer dashboard (app "eddy-dev"). Paste the sections the form asks for; keep this file in sync with what was submitted.

---

## Summary

eddy is a free, open-source macOS menu bar app. When someone clicks a Spotify link in another app (Messages, Mail, Slack…), eddy finds the same track or album on TIDAL and opens it **in the official TIDAL desktop app**. Its purpose is to bring listeners who receive Spotify links to TIDAL instead of Spotify.

- Non-commercial: free, no ads, no paid features, no data collection. Licensed MIT OR Apache-2.0.
- Source code: https://github.com/RespectWood/eddy
- Privacy notice: https://github.com/RespectWood/eddy#privacy (also linked next to "Connect Tidal" in the app, before signing in)
- Platform: macOS 13 and later.

## How eddy uses the TIDAL API

**Authorization:** each user signs in with their own TIDAL account through the Authorization Code flow with PKCE (`login.tidal.com/authorize`, redirect URI `eddy://auth/tidal`). eddy is a public client: no client secret ships with the app. **No scopes are requested.** Tokens are stored only in the macOS Keychain on the user's Mac, and users can disconnect at any time (Settings → Accounts → Disconnect), which deletes them.

**Endpoints (catalog reads only, with the user's token and their country):**

| Endpoint | Purpose |
|---|---|
| `GET /v2/tracks?filter[isrc]=…&include=albums,artists` | Find the track by ISRC (at most 3 pages) |
| `GET /v2/searchResults?filter[query]=…&include=tracks…` | Find the track by artist and title when the ISRC alone isn't enough |
| `GET /v2/albums?filter[barcodeId]=…` | Find an album by UPC |
| `GET /v2/albums?filter[id]=…&include=artists` | Album artists, to prefer an artist's original album over compilations (batches of 20 IDs) |
| `GET /v2/users/me` | Once after sign-in: the account country, for lookups. Without the `user.read` scope this returns nothing and eddy uses the Mac's region. No other profile field is kept (no username, email or name) |

ISRCs and UPCs come from Deezer's public API, matched by the Spotify title, artist and duration.

**Volume:** only on a user action: clicking a Spotify link, or copying a link to the song they're playing (also looked up when they open eddy's menu, so the link is ready; exact matches are then cached). An uncached track takes about 2–7 requests, an album 1. There is no background polling, crawling or bulk retrieval.

**What eddy does with the results:** it opens `tidal://track/<id>`, `tidal://album/<id>` or `tidal://artist/<id>` in the TIDAL desktop app (or `tidal.com/browse/…` if the app isn't installed). Without a match, it opens TIDAL's search. eddy shows no TIDAL content itself and plays nothing: **all playback happens in TIDAL's own app.**

**Storage:** each match (TIDAL ID, album title, album artist) is cached on the user's Mac for at most 30 days, so a link clicked again opens instantly without new API calls. Expired entries are deleted. Nothing is stored on any server; eddy has none.

## Attribution

Settings → Accounts shows the official TIDAL logo (horizontal, black in light mode and white in dark mode, with the clear space from the Design Guidelines, about 14 pt tall) linking to tidal.com, only in the TIDAL account row, never next to other services. eddy displays no TIDAL artwork or audio and doesn't alter any metadata; "TIDAL" is written in capitals throughout the app.

We took the logo from the Design Guidelines page. **Is there an official logo pack** (SVG or PNG, light and dark) we should use instead?

## Features we'd like you to confirm

Two optional features read from or press buttons in the TIDAL desktop app through macOS Accessibility. We want to be upfront about them and will turn them off for TIDAL if you'd rather we didn't offer them:

1. **Autoplay** (off by default). TIDAL deep links open a track's page but can't start playback, so after opening the link eddy presses that page's Play button in the TIDAL app, once, for the track the user clicked. It needs the user to grant the Accessibility permission.
2. **Copy Link for Current Song** (menu command). Reads the title and artist of the song playing in the TIDAL app (when the user asks, or when they open eddy's menu so the link is ready) and copies a link to it, to share with a friend. The submenu lists **TIDAL first**, then Apple Music and Spotify for friends on other services; for TIDAL users the keyboard shortcut copies the TIDAL link (`tidal.com/browse/track/…`, found with the same catalog lookup as above).

## Contact

RespectWood on GitHub: https://github.com/RespectWood/eddy (the form's contact field has the email address)
