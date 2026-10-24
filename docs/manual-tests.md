# Manual tests: v0.1 alpha

These checks need eyes on the screen, system dialogs or real apps, so they can't be automated from the terminal. Run them before tagging `v0.1.0-alpha`.

Mark each line ✅ / ❌. For ❌, note what happened instead.

## Already verified automatically

So you don't need to repeat these:

- Link parser: every URL shape in ticket 0001 (unit tests + fixture corpus); `parse()` runs in µs.
- Spotify → Apple Music lookups (live): tracks (incl. "– Remastered 2011" titles) and albums.
- Spotify → Tidal without credentials returns a Tidal search link (live).
- Deezer ISRC lookup (live).
- `Eddy.app` builds as a universal binary (arm64 + x86_64), signs ad-hoc, and launches as a background menu bar process.
- Sending a Spotify link to the running app: it resolves, caches the result, saves it to recents, and Music.app launches.
- Sending an ordinary link to the app: no crash or loop. Whether Firefox actually showed the page is test C1 below.

## Before you start

```sh
scripts/build-app.sh                       # → dist/Eddy.app
cp -R dist/Eddy.app /Applications/   # launch at login works best from /Applications
```

To watch what the app does while testing, run this in a separate terminal:

```sh
log stream --level info --predicate 'subsystem == "com.respectwood.eddy"'
```

To start over with a clean first run: quit Eddy from its menu, then run
`defaults delete com.respectwood.eddy; rm -rf ~/Library/Application\ Support/eddy`.

## A. Launch and menu bar

| # | Do this | Expected | Result |
|---|---|---|---|
| A1 | Open `/Applications/Eddy.app` | An icon (⚠︎ circle, since it isn't set up yet) appears in the top menu bar. **No Dock icon**, not in Cmd-Tab | |
| A2 | Same launch | The setup window opens and comes to the front | |
| A3 | Check the icon in light and dark mode (System Settings → Appearance) | Icon is legible in both | |
| A4 | Open the app a second time from Finder while it's running | Still **one** menu bar icon. The setup window comes to the front | |
| A5 | Click the menu bar icon | Menu groups, divided by lines: status line, "Not active: set up Eddy…", Disable Eddy / Copy Link for Current Song ▸ (Tidal, Apple Music, Spotify), Recently Opened / Open Spotify Links In, Start Playing Automatically / Open Other Links In, Launch at Login / Settings…, About, Quit | |

## B. Setup and default link handler

| # | Do this | Expected | Result |
|---|---|---|---|
| B1 | In the setup window, open "Open Spotify links in" | Apple Music and Tidal listed. Neither says "not installed" on this Mac | |
| B2 | "Open other links in" | Lists your browsers (Firefox, Safari, …), **not** Eddy itself. Firefox (your previous default) is preselected | |
| B3 | Click "Use Eddy for links" | macOS asks to confirm changing the default web browser to Eddy. **Note whether it asks once or twice** (http and https are set separately) | |
| B4 | Confirm | The window shows "Eddy receives links" ✓. The menu status changes to "Active: Spotify → …" and the icon changes to the Eddy mark (an e with an arrow) | |
| B5 | System Settings → Desktop & Dock → Default web browser | Shows Eddy | |
| B6 | First time a notification would be sent (or when the setup window opens) | macOS asks for notification permission. Allow it | |
| B7 | Click "Done", then reopen via menu → Settings… | Settings are remembered | |

## C. Ordinary links still work (most important)

| # | Do this | Expected | Result |
|---|---|---|---|
| C1 | Click `https://example.com` in Notes or Messages | Opens in Firefox **with no noticeable delay** | |
| C2 | Click 10–20 different ordinary links from Slack, Mail and Messages | All open in Firefox, nothing lost, no lag | |
| C3 | Change "Open other links in" to Safari, click a link | Opens in Safari | |
| C4 | Double-click a local `.html` file in Finder | Opens in the chosen browser (or its usual app). Not lost | |
| C5 | Click a link inside Firefox itself | Stays in Firefox, Eddy not involved (expected) | |

## D. Spotify → Apple Music

Set Open Spotify Links In → Apple Music. Test links:

- Track: `https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b?si=test` (The Weeknd – Blinding Lights)
- Track with a locale prefix: `https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC`
- Album: `https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv` (Pink Floyd – The Dark Side of the Moon)
- Remastered title: `https://open.spotify.com/track/7tFiyTwD0nx5a1eklYtX2J` (Queen – Bohemian Rhapsody – Remastered 2011)

| # | Do this | Expected | Result |
|---|---|---|---|
| D1 | Click the track link in Messages, with Music **running** | Music comes to the front **showing that exact song** (not just the app's home screen) | |
| D2 | Quit Music, click the track link again (**cold start**) | Music launches and shows the song. Note how many seconds it takes | |
| D3 | Album link | Music shows the **album** page, not a single song | |
| D4 | Locale-prefix and remastered links | Correct songs | |
| D5 | Does Music start **playing**, or only show the item? | Write down which. Either is OK for v0.1 (open question in PLAN.md) | |
| D6 | Click the same track link again | Opens instantly (cached) | |
| D7 | Menu → Recently Opened | Shows "The Weeknd – Blinding Lights" etc. Clicking "Open in Apple Music" reopens it. "Open Original Link" opens the Spotify page in Firefox | |

## E. Spotify → Tidal

Set Open Spotify Links In → Tidal. Deep link formats were checked against Tidal 2.43.2's source: `tidal://track/<id>`, `tidal://album/<id>` and `tidal://search?q=word+word`. Tidal runs `encodeURI` on the link, so the search query must not contain `%` escapes. These tests confirm the formats in the real app.

**Without API credentials** (the default):

| # | Do this | Expected | Result |
|---|---|---|---|
| E1 | Click the Blinding Lights track link, with Tidal running | Tidal comes to the front with a **search for "The Weeknd blinding lights"**. A notification says exact matching needs Tidal API credentials | |
| E2 | Quit Tidal, click again (cold start) | Tidal launches **without freezing** and shows the search once its window is ready | |
| E3 | In Terminal: `open "tidal://search?q=The+Weeknd+blinding+lights"` | Same as E1 | |
| E3b | In Tidal, Share → Copy track link on any song (gives `https://tidal.com/browse/track/<id>`), then `open "tidal://track/<id>"` | Tidal shows that track. This confirms the format used for exact matches | |

**With API credentials.** Create a free app at developer.tidal.com, then paste the Client ID and secret in Settings → "Tidal API credentials":

| # | Do this | Expected | Result |
|---|---|---|---|
| E4 | Click the Blinding Lights track link | Tidal shows **that exact track** | |
| E5 | Album link (Dark Side of the Moon) | Tidal shows the album | |
| E6 | In Terminal, from the repo: `cargo run -q -p eddy-cli -- resolve "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b" --to tidal --country SE`, with `TIDAL_CLIENT_ID` and `TIDAL_CLIENT_SECRET` set | Prints a `tidal://track/…` URL. If it prints an error, copy it here; the Tidal API call is untested | |
| E7 | `open "tidal://track/<id from E6>"` | Tidal shows the track | |

**Right release and landing page (ticket 0002).** Needs API credentials:

| # | Do this | Expected | Result |
|---|---|---|---|
| E8 | Click `https://open.spotify.com/track/5rb9QrpfcKFHM1EUbSIurX` (USHER – Yeah!) | Tidal opens **Confessions (Expanded Edition)** with the song highlighted, not a "Various Artists" compilation | |
| E9 | Click `https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC` (Rick Astley) | Opens **Whenever You Need Somebody** | |
| E10 | Menu → **Show Artist Page Instead of Song** on, click the Usher link again | Tidal opens **Usher's artist page**. Turn it off afterwards | |
| E11 | Menu → Recently Opened | Shows the conversions. "Open in Tidal" reopens the same page as the original click | |

## F. Other Spotify links and fallbacks

| # | Do this | Expected | Result |
|---|---|---|---|
| F1 | Playlist link `https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M` | Opens unchanged in Firefox | |
| F2 | Podcast episode link `https://open.spotify.com/episode/512ojhOuo1ktJprKbVcKyQ` | Opens unchanged in Firefox | |
| F3 | Artist link `https://open.spotify.com/artist/0gxyHStUsqpMadRV0Di1Qt` | Opens unchanged in Firefox (artist conversion is a later ticket) | |
| F4 | A **real** short link: in Spotify, Share → Copy link on a song (gives `spotify.link/…` or `open.spotify.com/…`), paste it in Messages to yourself, click it | Converts like a normal track link. **Short links are untested with real data**, so this one matters | |
| F5 | `spotify:track:0VjIjW4GlUZAMYd2vXMi3b` typed in Safari's address bar or `open` in Terminal | The Spotify app still handles these (Eddy doesn't take the `spotify:` scheme from an installed Spotify app). Expected: Spotify opens | |
| F6 | A track that isn't on Apple Music (e.g. a Spotify-exclusive podcast song or a local indie upload) | Music opens a **search**, plus a notification "Not found on Apple Music" | |

## G. Where links come from

Click a Spotify track link in each app and note whether Eddy converts it:

| # | Source | Converted? | Result |
|---|---|---|---|
| G1 | Messages | | |
| G2 | Slack (desktop app) | | |
| G3 | Discord (desktop app) | | |
| G4 | Mail | | |
| G5 | Notes | | |
| G6 | WhatsApp desktop | | |
| G7 | Any app that **doesn't** use the default browser (e.g. links in some Electron apps) | List them; known limitation | |

## H. Menu controls

| # | Do this | Expected | Result |
|---|---|---|---|
| H1 | Menu → Disable Eddy | Icon dims, status says "Disabled: links open as usual". Spotify links now open in Firefox unchanged, no autoplay | |
| H2 | Menu → Enable Eddy | Conversion works again | |
| H3 | Menu → Open Spotify Links In → switch | Next Spotify link goes to the new service | |
| H4 | Menu → About | About panel with name and version 0.1.0 | |
| H5 | Menu → Quit | Icon disappears. **Note:** links then fail to open until Eddy runs again or the default browser is changed back (expected for v0.1) | |

## I. Launch at login, offline, notifications

| # | Do this | Expected | Result |
|---|---|---|---|
| I1 | Menu → Launch at Login → on. Log out and back in (or reboot) | Menu bar icon appears automatically | |
| I2 | System Settings → General → Login Items | Eddy listed | |
| I3 | Wi-Fi off, click an ordinary link | Opens in Firefox (shows Firefox's offline page). No delay from Eddy | |
| I4 | Wi-Fi off, click an uncached Spotify link | Original link opens in Firefox, plus a notification "Couldn't convert the link" | |
| I5 | Notifications from E1/F6/I4 | Appear as banners with readable text | |

## K. Autoplay (ticket 0003)

Menu → **Start Playing Automatically** on. macOS asks for the Accessibility permission the first time; grant it.

| # | Do this | Expected | Result |
|---|---|---|---|
| K1 | Tidal open, click the Usher – Yeah! link | Tidal opens *Confessions* and **Yeah! starts playing** within ~2 s | |
| K2 | Click the same link again while it plays | Keeps playing (doesn't pause or restart) | |
| K3 | **Quit Tidal**, click the link | Tidal launches, opens the album and the song plays (may take 5–15 s) | |
| K4 | Album link `https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv` | Album plays from track 1 | |
| K5 | Menu → Show Artist Page Instead of Song on, click a link | Artist page opens, nothing plays (autoplay needs the song's page) | |
| K6 | Apple Music (needs a subscription): Open Spotify Links In → Apple Music, click the Usher link | Music opens the album and *Yeah!* plays. The pointer jumps to the row for a moment and comes back | |
| K7 | Autoplay off | Nothing plays, no permission prompt | |

If something fails: `~/Library/Application Support/eddy/debug/autoplay.log` shows each attempt once debug commands are enabled (`scripts/debug.sh` enables them on first use; `scripts/debug.sh off` disables them).

## L. Connect Tidal (ticket 0004)

One-time setup: in developer.tidal.com → your app, add the redirect URI `eddy://auth/tidal`.

| # | Do this | Expected | Result |
|---|---|---|---|
| L1 | Settings → Accounts → **Connect Tidal** | Tidal's login page opens in your normal browser (already logged in if it remembers Tidal) | |
| L2 | Log in; allow the browser to open Eddy | Row says "Connected" (with name and country only if `user.read` is allowed for the app) | |
| L3 | Clear the Developer client ID/secret fields, click the Usher link | Exact match (*Confessions*) and autoplay, using your login | |
| L4 | Quit and restart Eddy | Still connected | |
| L5 | **Disconnect**, click the Usher link | Tidal shows search results + "Connect Tidal" notification | |
| L6 | Click Connect Tidal, then **Cancel** in Settings without logging in | Row goes back to "Connect Tidal", no error | |
| L7 | Revoke Eddy's access in your Tidal account settings, wait an hour (token expiry) and click a link | Search results + "Your Tidal login expired" notification; row shows **Reconnect Tidal** | |

## O. Copy Link for Current Song (ticket 0009)

Needs the Accessibility permission (as for autoplay). Tidal's window must be on the current desktop. The submenu **Copy Link for Current Song** lists Tidal, Apple Music and Spotify; ⇧⌘C is on your own service (Open Spotify Links In). O1–O7 copy Spotify links.

| # | Do this | Expected | Result |
|---|---|---|---|
| O1 | Play *Yeah!* in Tidal, menu → **Copy Link for Current Song → Spotify**, paste into Messages | Notification "Copied Spotify link"; the pasted link opens *Yeah!* on Spotify | ✅ (clipboard verified, 2026-10-23) |
| O2 | Same with a niche song (e.g. Robyn – Dancing On My Own) | "Copied Spotify search link"; the link opens a Spotify search for the song | |
| O3 | Pause the song, copy again | Still copies the paused song | |
| O4 | Close Tidal's window (or move it to another desktop), copy | "Can't see Tidal's player" notification, clipboard unchanged | |
| O5 | Quit Tidal and Music, copy | "Nothing is playing" notification | |
| O7 | Play a song never copied before. Open the menu, wait ~2 s, click Copy | Link copied right away (log: "prefetched, ready 0.0Xs after the click"). Just opening and closing the menu leaves the clipboard unchanged | |
| O6 | Music playing (any song, library or catalog) | First time: macOS asks to let Eddy control Music; then the link is copied | |
| O8 | Service Tidal, Tidal connected. Play *Yeah!*, menu → **Copy Link for Current Song → Tidal** (⇧⌘C) | "Copied Tidal link"; the link (`tidal.com/browse/track/…`) opens *Yeah!* | |
| O9 | Same, **Copy Link for Current Song → Apple Music** | "Copied Apple Music link"; the link (`music.apple.com/…?i=…`) opens *Yeah!* | |
| O10 | Disconnect Tidal, copy the Tidal link again | "Copied Tidal search link"; the link opens a Tidal search | |
| O11 | Switch Open Spotify Links In to Apple Music, open the submenu | Still Tidal, Apple Music, Spotify in that order; ⇧⌘C is now shown on Apple Music | |

## J. Undo

| # | Do this | Expected | Result |
|---|---|---|---|
| J1 | System Settings → Desktop & Dock → Default web browser → Firefox | Links go straight to Firefox again. Menu shows "Not active" | |
