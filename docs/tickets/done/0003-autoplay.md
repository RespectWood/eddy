# 0003 — Autoplay: start the linked song automatically

| | |
|---|---|
| **Status** | Tidal: implemented and verified through the full click flow. Cold start: manual test. Apple Music: implemented, untested (no subscription on the dev Mac) |
| **Phase** | 1 |
| **Depends on** | 0002 |

## Problem

Tidal and Apple Music links can only **navigate** (see 0002). After Eddy opens the right page, the user still has to press play.

## Why there's no API for this

- `tidal://` deep links have no play command (checked in TIDAL 2.43.2's link handler and web client).
- The TIDAL SDK's Player module plays audio **inside the host app**, can't control the TIDAL app, and only gives third parties 30-second previews ([web SDK #133](https://github.com/tidal-music/tidal-sdk-web/issues/133)).
- Music's AppleScript can only play tracks in the user's library, not catalog tracks.

## Solution: press the page's play control via Accessibility

After the link opens the app, Eddy reads the app's accessibility tree, waits for the linked page, presses the matching **play** control and confirms in the player bar that the right song is playing. Opt-in setting: **"Start playing automatically"**. It needs the macOS **Accessibility** permission.

### Tidal (Electron, TIDAL 2.43.2)

- Web content only appears in the accessibility tree after setting `AXManualAccessibility`. In a **freshly launched** Tidal it's then only reachable *upwards*: a hit test lands inside the page and its `AXParent` chain leads to the `AXWebArea`, but the window's `AXChildren` never list it. (Once Tidal has been running a while, both directions work, which is why cold starts failed at first.) Eddy tries the children first, then hit test + parents.
- Cold starts can take a while (launch + login), so autoplay waits up to 45 s.
- **Page loaded:** the `AXWebArea` title is `"<album> / <artist> - TIDAL"`.
- **Track link:** in `dom#=main` → `AXTable` (Tracklist) → the `AXRow` with class `_tableRowSelected_…` (the row the link highlighted) → its `AXButton` with class `_playButton_…`. The row title is checked as well.
- **Album link:** the header's `AXButton` with classes `_marketButton_…` + `_primary_…`.
- **Verification:** `dom#=footerPlayer` → `_trackContent_…` → first `AXLink` = title of the song now playing.
- Class names are matched by **prefix** (the hash suffix changes per build). Button labels aren't matched because they're localized.
- If the linked song is already the current song, nothing is pressed (pressing again would pause it).
- **Results (Tidal open, full click flow via `debug/click`):** Usher – *Yeah!* and The Weeknd – *Blinding Lights* verified playing 1.8–2.2 s after the click. *The Dark Side of the Moon* album: Play pressed on the first attempt.
- **Several clicks in a row:** lookups finish in any order (cache hits are instant), so only the **most recent click** autoplays, and a newer autoplay cancels an older one that's still waiting.

### Apple Music (macOS 15)

- Track rows: `AXGroup id="Music.shelfItem.AlbumTrackLockup[id=track-lockup-<album>-<track>…"`, which contains the **exact catalog IDs** from the link (`/album/…/<album>?i=<track>`).
- Album header: `AlbumDetailHeaderLockup[id=album-detail-header-<album>…`. Play is the first button whose title equals its description.
- An AX press on a track row only **selects** it. Return sent to the process and process-targeted double-clicks didn't start playback, so tracks use a real double-click. It only happens when Music is frontmost and the point is inside Music's window, and it puts the pointer back afterwards.
- ⚠️ **Unverified:** the development Mac has no Apple Music subscription (confirmed), so catalog songs can't play there at all; even Music's own album Play button does nothing. Needs testing on a Mac with a subscription.

## Development setup

- **Signing:** a local self-signed "openstream Dev" certificate (login keychain) keeps the signature, and with it the Accessibility permission, stable across rebuilds. `scripts/build-app.sh` uses it automatically.
- **Debug commands** (only with `defaults write com.respectwood.eddy debugCommands -bool true`):
  - `open "eddy://debug/dump?app=tidal|music"` → `~/Library/Application Support/eddy/debug/ax-<app>.txt`
  - `open "eddy://debug/autoplay?app=…&kind=track|album&title=…&artist=…&album=…&url=…"`
  - `open "eddy://debug/click?url=<spotify link>"` → the full flow, as if the link had been clicked
  - Each failed attempt run ends with the reason in the log, e.g. `page is "…", waiting for "<album> / …"`
  - Log: `debug/autoplay.log`

## Known limitations

- Autoplay is skipped when Tidal tracks are set to open on the **artist page**.
- Breaks if Tidal renames the CSS classes or Music changes its identifiers. The debug dump makes fixing that a quick job.
- Labels like "Play" are localized, so the player-state check reads only the song title, never button text.

## Acceptance criteria

- [x] Tidal, app open: linked track plays, verified in the player bar.
- [ ] Tidal, cold start: linked track plays after Tidal launches (manual test K3; the dev sandbox can't launch Tidal).
- [x] Tidal album link plays the album.
- [x] Only the most recent click autoplays.
- [ ] Apple Music track and album (needs a subscription).
- [ ] Turning autoplay on asks for the Accessibility permission; with it off, Eddy never asks.
