# 0005 — Connect Apple Music: exact matches through MusicKit

| | |
|---|---|
| **Status** | Open |
| **Phase** | 2 |
| **Estimate** | ½-day spike + 1–2 days |
| **Depends on** | 0004 (shared account design: Accounts section, `ResolveError::AccountExpired`, setup-flow step) |
| **Requires** | Paid Apple Developer Program membership |

## Problem

Apple Music lookups use the open **iTunes Search API** and match by title, artist and duration. That works for most songs, but:
- There's **no ISRC matching**, so it's weaker on remasters, live versions, clean/explicit versions and compilations.
- The **region** comes from the Mac's settings, not the user's Apple Music storefront, so songs available in the user's store can be missed (and vice versa).

## Goal

**Connect Apple Music** in Settings → Accounts (introduced in 0004). After connecting, Eddy finds the **exact** recording by ISRC through Apple's official API, in the user's own storefront, and ranks releases the same way as Tidal (original album over compilations). Not connecting is always allowed: Eddy then uses iTunes Search as today.

```
Settings → Accounts
  Apple Music   [ Connect Apple Music ]  →  system prompt  →  ✓ Connected (store: Sweden)   [Disconnect]
```

## How it works (MusicKit)

Apple's **MusicKit** framework (macOS 12+) talks to the Apple Music API with a developer token that Apple **generates automatically** for apps whose App ID has the MusicKit service enabled, so there's no key or secret in the app. Connecting is a system permission prompt ("Eddy would like to access Apple Music"), not a web login.

1. `MusicAuthorization.request()` → the system prompt. macOS keeps the status; Eddy stores no tokens.
2. **Exact matching:** `MusicCatalogResourceRequest<Song>(matching: \.isrc, equalTo: isrc)` with the ISRC from Deezer (as for Tidal); albums by UPC (`\.upc`). Rank the results with the existing release ranking (`matching::release_score`).
3. **Storefront** (region) from `MusicDataRequest.currentCountryCode`.
4. Open the result in Music.app as today (`music://music.apple.com/<storefront>/album/…?i=…`), so autoplay (0003) keeps working.
5. Fallback when not connected or no ISRC match: today's iTunes Search fuzzy match.

MusicKit is a Swift framework, so this lookup lives **in the Swift app**, not the Rust core. The resolver gets an `AppleMusicCatalog` **callback interface** (UniFFI) that the app implements with MusicKit; the Rust core does the ranking and builds the URLs. The CLI keeps using iTunes Search.

## Requirement: Apple Developer Program

MusicKit has to be enabled for the app's **App ID** in the Apple Developer portal, and the app has to be signed with that team. The local "openstream Dev" certificate isn't enough. This is the same paid account needed anyway for Developer ID signing and notarization (PLAN.md Phase 3), so it makes sense to do both together.

## Spike first (½ day): open questions

Answer these before building and write the results into this ticket.

- [ ] Does MusicKit's automatic developer token work for a **Developer ID app distributed outside the App Store** (MusicKit service on the App ID, no App Store)?
- [ ] Do **catalog requests** need `MusicAuthorization` at all, or only user-library requests? If not, "Connect" may only be needed for the storefront, and exact matching could work without any prompt.
- [ ] Do catalog lookups work **without an Apple Music subscription** (the development Mac has none)?
- [ ] Is ISRC/UPC filtering available through MusicKit's native requests on macOS 13, or only through `MusicDataRequest` against the REST endpoint (`/v1/catalog/{storefront}/songs?filter[isrc]=…`)?
- [ ] What do catalog results include for ranking: album artist ("Various Artists" detection), release date, popularity-like signals?

If MusicKit requires the App Store, re-plan (alternative: a signed developer token from a small backend, see PLAN.md §3).

## Plan

- [ ] App ID with the MusicKit service; entitlement and signing set up in `scripts/build-app.sh`; documented in the README.
- [ ] `AppleMusicAccount.swift`: `MusicAuthorization` status/request, storefront, Connect/Disconnect in the Accounts UI. "Disconnect" opens System Settings → Privacy & Security → Media & Apple Music, since apps can't revoke their own authorization.
- [ ] `AppleMusicCatalog` callback interface: `songs(isrc) / albums(upc) → [CatalogItem { id, title, artist, album, album_artist, release_date, url }]`, implemented with MusicKit.
- [ ] Core: when the callback is available, use ISRC/UPC + release ranking; otherwise iTunes Search as today. Unit tests with recorded catalog results.
- [ ] Storefront feeds the lookup country (shared design from 0004).
- [ ] `ResolveError::AccountExpired { service: AppleMusic }` when authorization is revoked → "Reconnect Apple Music".

## Security & privacy

- No secrets in the app: Apple generates the MusicKit developer token.
- Only catalog reads; no access to the user's library or playlists.
- Users can revoke access in System Settings → Privacy & Security → Media & Apple Music.

## Acceptance criteria

- [ ] Spike questions answered in this ticket.
- [ ] After Connect Apple Music, the Usher link resolves by ISRC to *Confessions (Expanded Edition)* in the account's storefront.
- [ ] The 10-track no-compilations live test (`tests/fixtures/original_releases.txt`) passes for Apple Music through MusicKit.
- [ ] Not connected / revoked → iTunes Search fallback; "Reconnect Apple Music" shown after revoking.
- [ ] Opening in Music.app and autoplay (0003) still work with MusicKit results.
- [ ] Manual tests added (section L).
