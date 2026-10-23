# 0007 — Catch Spotify links clicked inside the browser (Firefox extension)

| | |
|---|---|
| **Status** | Open |
| **Phase** | 2 |
| **Estimate** | ½-day spike + 2–3 days (Firefox); Chrome/Safari later |
| **Depends on** | 0001 |

## Problem

openstream only sees links that macOS hands to the default "browser": clicks in Messages, Slack, Mail and other apps. A Spotify link clicked **on a web page inside Firefox** (Gmail, web Slack, Reddit, Discord in the browser…) never leaves Firefox, so it opens Spotify's web player as before. For someone who lives in the browser, that's a large share of shared links.

## Goal

A small **Firefox extension** that hands Spotify links clicked in Firefox to openstream, which then handles them exactly like a click from Messages: right release, native app, autoplay.

## Design

### How the extension talks to openstream: native messaging (preferred)

Firefox's **native messaging** lets an extension send JSON to a local program registered by the user. No prompts, and web pages can't use it.

1. The extension sends `{ "url": "https://open.spotify.com/track/…" }` to a native host.
2. The host is a tiny command-line program bundled in the app (`openstream.app/Contents/MacOS/openstream-browser-host`). It opens the URL **with openstream** (`NSWorkspace.open([url], withApplicationAt: openstream.app)`), so openstream receives it as a normal link.
3. openstream registers the host by writing the manifest to `~/Library/Application Support/Mozilla/NativeMessagingHosts/app.openstream.browser.json` (a Settings toggle: "Firefox integration"), with `allowed_extensions` set to the extension's ID only.

**Alternative** (fallback if native messaging is too fiddly): the extension navigates to `openstream://open?url=…`. That's simpler, but Firefox asks "Allow this site to open openstream?" per site, and *any* web page could trigger it, so openstream would have to drop autoplay for these links to avoid pages starting music. Decide in the spike.

### What the extension intercepts

- Clicks on links to `open.spotify.com/{track,album,artist,playlist,episode,show}/…`, `spotify.link/…`, `spoti.fi/…` on **other** sites: the top-level navigation is cancelled and the URL sent to openstream.
- **Not** intercepted: navigations *within* open.spotify.com (the user is using the web player on purpose), URLs typed into the address bar, links opened with a modifier key (⌥-click = "open normally"), and link types openstream doesn't convert (`episode`, `show`, depending on what openstream reports).
- Both clicks and redirects (e.g. Google search results, t.co and other link shorteners) via `webNavigation.onBeforeNavigate` / `webRequest`, top frame only.

### Avoiding loops

When openstream is **disabled** (or doesn't convert the link type), it forwards the Spotify link to the browser, which is Firefox, and the extension would catch it again. openstream marks links it forwards with a fragment (`#openstream-pass`), which the extension ignores (and strips). Alternatively the host can answer "openstream is disabled" and the extension lets the navigation through. Decide in the spike.

### Distribution

- Firefox requires extensions to be **signed by Mozilla**, also when distributed outside the add-ons site ("unlisted" signing is free and quick). Developer builds can be loaded temporarily via `about:debugging`.
- Source lives in `extensions/firefox/` (Manifest V3, plain JS, no build step).
- Chrome and Safari can reuse most of it later (Chrome uses the same native messaging; Safari needs a Safari Web Extension inside the app bundle).

## Spike first (½ day)

- [ ] Does native messaging work with a host binary inside an app bundle (paths, quarantine, signing)?
- [ ] Can `webNavigation`/`webRequest` cancel a top-level navigation cleanly in MV3 Firefox, without a leftover blank tab?
- [ ] Which loop guard is simplest: fragment marker or host status reply?
- [ ] Unlisted signing on addons.mozilla.org: account and steps.

## Plan

- [ ] `extensions/firefox/`: manifest (MV3, `nativeMessaging`, `webNavigation`, host permissions for Spotify domains only), background script, options page with an on/off switch.
- [ ] Native host target in the Swift package (`openstream-browser-host`), copied into the app bundle by `scripts/build-app.sh`.
- [ ] openstream Settings: **Browser integration → Firefox** toggle that installs/removes the native-host manifest, with a link to install the extension.
- [ ] Loop guard on openstream's forwarding.
- [ ] Debug log lines for links received from the browser.
- [ ] README section; manual tests (section M).

## Security & privacy

- The extension only has permissions for Spotify's domains; it never sees other browsing.
- Native messaging is limited to the openstream extension's ID; web pages can't call it.
- Nothing is sent anywhere except to openstream on the same Mac.

## Acceptance criteria

- [ ] A Spotify track link clicked in Gmail/web Slack in Firefox opens the right release in Tidal and autoplays (if on), with no prompt.
- [ ] Using open.spotify.com directly, or typing a Spotify URL, still works normally.
- [ ] ⌥-click opens the link normally.
- [ ] openstream disabled → the link opens in Firefox's Spotify web player, no loop.
- [ ] Extension signed (unlisted) and installable; Settings toggle installs/removes the native host.
- [ ] Manual tests added (section M).
