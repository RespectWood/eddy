# openstream — Rundown, Analysis & Development Plan

> Status: idea / pre-MVP · Platform focus: macOS (mobile later) · Date: 2026-10-22

---

## 1. Rundown

**Problem.** User A shares a Spotify link (`https://open.spotify.com/track/...`). User B uses Apple Music or Tidal. When User B clicks the link, it opens Spotify's web player, which doesn't help them. They then have to copy the artist and title and search for the track in their own app by hand.

**Desired result.** When User B clicks a Spotify track link anywhere on their Mac (Messages, Slack, Discord, Mail, a browser), the same track opens in **their preferred streaming app** (Apple Music or Tidal). They don't have to do anything else.

**One-liner.** *A small macOS app that sits between your link clicks and your browser, and turns Spotify links into Apple Music or Tidal links.*

---

## 2. Analysis

### 2.1 What has to happen when a link is clicked

```
click link ──► [1] intercept ──► [2] parse ──► [3] resolve ──► [4] open in target app
                    │                                 │
                    └── not a music link ──► forward to real browser
                                                      └── no match ──► fallback (search in target app / open original)
```

1. **Intercept.** Catch the URL before Spotify or the browser handles it.
2. **Parse.** Find the content type (track, album, artist, playlist, episode) and its Spotify ID.
3. **Resolve.** Get the track's metadata (ISRC, title, artist, duration) and find the same item in the target catalog.
4. **Open.** Open the target app at that exact item.

### 2.2 Interception options on macOS

| Approach | How it works | Coverage | Effort | Verdict |
|---|---|---|---|---|
| **A. Act as the default browser** (like Velja, Choosy, Finicky) | Register as the `http`/`https` handler. Handle Spotify URLs and forward everything else to the user's real browser. | Very high. Messages, Slack, Discord, Mail and most other apps send links to the default browser. | Medium | ✅ **Primary approach** |
| B. Register the `spotify:` URL scheme | Claim `spotify:` URIs with `LSSetDefaultHandlerForURLScheme` | Low. Most shared links are `https://open.spotify.com`, not `spotify:` | Low | ➕ Add-on |
| C. Browser extension (Safari, Chrome) | Redirect navigations to `open.spotify.com` | Only links clicked inside the browser. Needs one extension per browser. | Medium | Later / optional |
| D. Clipboard watcher / Share menu / Services | The user copies a link or right-clicks it, then converts it | Manual. Doesn't meet the "just works" goal | Low | Fallback utility only |

**Why A is the right choice:**
- It's a proven pattern. Velja, Choosy, Finicky and Browserosaurus all do it.
- It works in every app that opens links through the system (`NSWorkspace` / LaunchServices).
- Non-music links pass through to the real browser unchanged. If forwarding is fast and reliable, the user won't notice the app is there.

**Caveats with A:**
- macOS shows a system confirmation dialog when the default browser changes. That's fine, it's a one-time onboarding step.
- Some apps open links in an embedded web view or ignore the default browser (some Electron apps, some in-app previews). Those links won't be caught. This is acceptable.
- If the user has the Spotify desktop app installed, Spotify may grab its own links in some flows. The app intercepts before any browser, so this mostly doesn't matter. It still needs testing.
- **Trust:** the app sees every link the user clicks. The app has to promise and show that it inspects URLs locally and sends only music links to any lookup service.

### 2.3 Link formats to handle

- `https://open.spotify.com/track/{id}?si=...` (strip query parameters)
- `https://open.spotify.com/intl-de/track/{id}` (locale prefix)
- `https://open.spotify.com/album/{id}`, `/artist/{id}`, `/playlist/{id}`, `/episode/{id}`, `/show/{id}`
- `spotify:track:{id}` (URI scheme)
- Short links: `https://spotify.link/...`, `https://spoti.fi/...`. Resolve these by following the redirect with a `HEAD`/`GET` request (no browser).

**MVP scope:** tracks, then albums. Artists are straightforward later. Playlists are a separate problem (see §2.6). Podcasts are passed through to the browser.

### 2.4 Resolving: Spotify item → Apple Music / Tidal item

**Best matching key: ISRC** (International Standard Recording Code). It identifies a specific recording and all three services expose it. For albums the equivalent is **UPC**.

| Option | How | Pros | Cons |
|---|---|---|---|
| **Odesli / song.link API** (`api.song.link/v1-alpha.1/links?url=`) | One call returns links for every platform | No auth for low volume, handles all platforms, fastest to MVP | Third-party dependency. Rate limit without a key (~10 req/min). Privacy: shared URLs go to a third party. Matching sometimes misses. |
| **Direct APIs** | Spotify Web API → ISRC, then Apple Music API `GET /v1/catalog/{storefront}/songs?filter[isrc]=…`, then TIDAL API (`openapi.tidal.com`) filter by ISRC | Accurate, no middleman, full control | Needs credentials for every service. Apple Music API needs a paid Apple Developer membership and a signed developer token. Spotify has tightened Web API access several times, so current dev-mode limits need checking. |
| **Metadata fallback** | Read `og:title` / `music:musician` meta tags from the `open.spotify.com` page, or use Spotify oEmbed, then text-search the target catalog (iTunes Search API is free and needs no auth) | Needs no Spotify credentials | Fuzzy matching. Remasters, live versions and clean vs. explicit versions get mixed up |

> **Update (2026-10-22):** Odesli's keyless public API is shut down (`401 PUBLIC_API_ACCESS_DEPRECATED`). v0.1 uses free sources instead: Spotify's public embed page for metadata, the **iTunes Search API** for Apple Music (fuzzy match on title, artist and duration), and **Deezer** (ISRC/UPC) + the **official TIDAL API** (free client credentials) for exact Tidal matches. Without Tidal credentials, openstream opens a search in the Tidal app. An Odesli API key is still an option for later.

**Recommended strategy (layered):**
1. **MVP:** use Odesli. It's quick to build and good enough to test whether the idea works.
2. **v1:** resolve directly by ISRC/UPC, with Odesli as a fallback (or the other way round). Put the credentials behind a small backend so no API secrets ship inside the app.
3. **Fuzzy fallback:** search by title + artist + duration (±2 s), then score the candidates.
4. **Last resort:** open the target app's **search** for "artist title", so the user still saves a step. If that fails too, offer to open the original link.

**Matching edge cases:**
- **Region/storefront:** a track can exist in the Apple Music US catalog but not in DE. Use the user's storefront (from MusicKit, or a setting).
- **One ISRC, several items:** the same recording can appear on a single, an album and a compilation. Prefer the album the Spotify link refers to (match by album UPC or name).
- **Explicit vs. clean** versions have different ISRCs. Respect the version that was shared.
- **Missing on the target service:** show a clear notification instead of failing silently.
- **Caching:** cache Spotify ID → target ID mappings locally (e.g. SQLite). Repeated links then open instantly and use fewer API calls.

### 2.5 Opening the target app

> **Hard requirement: if the user's streaming app is installed, the link MUST open in that native desktop app, never in the web player.** The browser or web player is only used when the app isn't installed, and the user should be told when that happens.

**How to detect whether the app is installed:** ask LaunchServices with `NSWorkspace.shared.urlForApplication(withBundleIdentifier:)` (`com.apple.Music`, Tidal's bundle ID, which needs confirming) and `urlForApplication(toOpen:)` for the `tidal://` / `music://` schemes. Check every time a link is opened, or cache the answer and refresh it when apps are installed or removed, so a newly installed app is used right away.

**Never hand an `https://` link to the system without a target app.** openstream is the default browser itself, so the link would loop back to it or end up in the web player. Always use a custom scheme (`tidal://`, `music://`) or open the URL with an explicit app (`open(_:withApplicationAt:)`).

- **Apple Music:** open the `https://music.apple.com/{storefront}/song/...` URL with `NSWorkspace.shared.open(_:withApplicationAt:configuration:)` pointed at `Music.app` (bundle `com.apple.Music`), or use the `music://` scheme. Check whether playback starts or the item is only shown. Starting playback may need AppleScript or MusicKit. The MVP only needs to show the item.
- **Tidal:** the desktop app registers the `tidal://` scheme (e.g. `tidal://track/{id}`). The exact formats need testing. If the desktop app isn't installed, fall back to `https://tidal.com/browse/track/{id}` in the browser.
- **Web fallback (only when the native app isn't installed):** open the web player URL in the user's real browser and show a one-time hint ("Install the Tidal app for the best experience").
- **App installed but not running:** launch it and wait for it to be ready before sending the deep link. Cold starts can drop the link, especially in Tidal (Electron), so test this and retry if needed.

### 2.6 Hard problems / out of MVP scope

- **Playlists:** these are made by users and exist only on Spotify. Converting one means creating a copy in the user's library (needs OAuth with Apple Music/Tidal, plus matching every track one by one). This is closer to what Soundiiz and TuneMyMusic do. Defer to a later phase as an optional "Import playlist" action.
- **Podcasts:** catalog coverage differs between services and there's no ISRC. Pass through to the browser.
- **Spotify exclusives:** they don't exist on the other services, so use the fallback UI.
- **Reverse direction:** Apple Music/Tidal → Spotify for friends. This is a natural extension of the same engine and fits the name "openstream".

### 2.7 Market scan (October 2026)

> Based on a web scan of App Store listings, reviews and vendor pages. Feature claims for third-party apps come mostly from their own listings and still need to be checked by installing them.

Existing solutions fall into five groups:

**A. Receiver-side redirectors: the closest competitors**

| Product | Platforms | Services | How it intercepts | Gaps |
|---|---|---|---|---|
| **Convusic** ($0.99) | iOS, iPadOS, macOS | Apple Music, Amazon, Deezer, Pandora, Spotify, **Tidal**, YouTube Music | Safari extension, or pasting links into the app | **Safari only.** Links open in Chrome/Arc/Firefox aren't caught. One review says links are intercepted only when the *sender's* app isn't installed. Rated 3.1/5 (17 ratings). Doesn't handle playlists. |
| **Euphony** (free) | iOS, macOS Safari | Spotify ↔ Apple Music | Safari extension | Safari only. **No Tidal.** |
| **MusicMatch** | iOS (Mac on Apple silicon only) | Spotify → Apple Music | Safari extension + share sheet | Safari only, one direction, Apple silicon only. Older app, may no longer be maintained. |

**B. Sender-side smart links: Odesli/song.link, Songwhip, SongPort, JamShare, Annie**
- The *sender* has to convert the link before sending it. In practice they almost never do, because Spotify's share button copies a plain `open.spotify.com` URL.
- These make a landing page where the recipient picks a service, which is one extra click and a web page.
- Odesli covers the most services (incl. Bandcamp, Audiomack, Amazon). That makes it a good **backend for openstream**, not a competitor.
- Feature.fm, Linkfire, etc. are marketing tools for artists and aren't relevant here.

**C. Built-in sharing from the services themselves**
- **Tidal** share links let the *recipient* choose a service such as Spotify. This only covers Tidal → others, and the chosen service opens in a browser, not its desktop app.
- **Spotify** has no reason to help users leave its platform, so it won't add anything like this.
- **Apple Music** can import playlists but doesn't convert links.

**D. Manual / power-user tools**
- **Raycast "Music Link Converter" extension:** copy a link, run a command, get the converted link. Manual.
- **Songboard** (Gumroad, macOS): watches the clipboard and turns copied music links into song.link URLs. Helps the sender, not the receiver.
- **songlink-cli**, iOS Shortcuts, Finicky with a custom script: possible for developers, but not a product.

**E. Link routers without music knowledge: Velja, Choosy, Finicky, Browserosaurus**
- They have the right interception mechanism (default-browser handler) but can't convert links between services. None of them supports music services out of the box.

**F. Playlist/library transfer: Soundiiz, TuneMyMusic, FreeYourMusic, SongShift, PlaylistGo**
- One-time library moves. They don't intercept links. These could be partners for a future "import this playlist" action.

### 2.8 Gap & USP

**What's missing on the market:** nobody combines *system-level interception* (category E) with *music link conversion* (categories A and B). All the existing automatic redirectors are **Safari extensions**, so they:
1. do nothing for people whose default browser is Chrome, Arc, Firefox or Brave, which is most desktop users;
2. depend on the link going through Safari at all;
3. barely support **Tidal**: only Convusic lists it, and it opens the result in the browser instead of the Tidal desktop app.

**Proposed USP:**

> **"Click any Spotify link, anywhere on your Mac, and it plays in your app."**
> openstream is the only receiver-side music link redirector that works at the **system level**. It works with any browser and any messaging app, and it opens the **native** Apple Music or Tidal desktop app. Set it up once and forget about it.

Supporting points, in order of strength:

| # | Differentiator | Why it matters | Who lacks it |
|---|---|---|---|
| 1 | **Browser-independent, system-level interception** (acts as the default browser and forwards all other links) | Works when links come from Messages, Slack, Discord, Mail or WhatsApp, whatever the user's real browser is | All category A apps (Safari only) |
| 2 | **First-class Tidal desktop support** (`tidal://` deep links, not the web player) | Tidal users are the most neglected group. Of the scanned apps, only Convusic lists Tidal at all. | Euphony, MusicMatch, all smart-link tools |
| 3 | **Zero action from the sender** | Fixes the real behavior: friends will keep sending plain Spotify links | All category B tools |
| 4 | **Native Mac experience:** menu bar app, opens instantly, history of conversions, "open original instead" | Existing tools are mostly iOS-first and ported to Mac | Convusic, MusicMatch |
| 5 | **Privacy and openness:** open source, URLs inspected locally, only music links leave the machine | A default-browser app has to earn trust. That's a reason to pick it over closed-source tools | Everyone (none are open source and system-wide) |
| 6 | **Any-to-any later** (Apple Music/Tidal → Spotify for friends, YouTube Music, Deezer) | The same engine grows into a general "music links just work" layer | — |

**USP risks / honesty check:**
- The moat is thin. Convusic or Velja could add the other half. Being first, focused and good is the realistic defense, and open source helps.
- Many users don't want to change their default browser. The onboarding has to explain this clearly, and an optional Safari/Chrome extension mode could serve users who refuse.
- The market is niche: macOS users who use Apple Music or Tidal and regularly receive Spotify links. That's fine for an open-source tool or a cheap paid utility, but not for a venture-scale business.

**Validation before building (add to Phase 0):**
- [ ] Install Convusic and Euphony and confirm the gaps listed above (non-Safari default browser, Tidal desktop, links from Messages/Slack).
- [ ] Check whether Velja/Choosy rules plus a script can already do this. If so, how much setup does it take?
- [ ] Ask in r/TIdaL, r/AppleMusic and r/macapps whether people want this. The search terms people use show real demand.

**Sources:** [Convusic (App Store)](https://apps.apple.com/us/app/id1591366129) · [Euphony (App Store)](https://apps.apple.com/app/id1639030623) · [MacRumors: Open Spotify links in Apple Music](https://www.macrumors.com/how-to/open-spotify-links-in-apple-music-vice-versa/) · [iMore: MusicMatch](https://www.imore.com/musicmatch-new-app-opening-spotify-links-apple-music-vice-versa) · [TechRadar: MusicMatch](https://www.techradar.com/news/musicmatch-makes-sure-spotify-or-apple-music-links-play-in-the-right-app) · [TechRadar: Tidal share links](https://www.techradar.com/audio/audio-streaming/tidals-game-changing-feature-lets-your-friends-open-shared-songs-in-spotify) · [SongPort: smart link comparison 2026](https://songport.link/blog/best-free-music-smart-links) · [Raycast Music Link Converter](https://www.raycast.com/Quentin23Soleil/music-link-converter) · [Songboard](https://wsig.gumroad.com/l/songboard) · [AlternativeTo: Songlink](https://alternativeto.net/software/songlink)

### 2.9 Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Users don't trust a "default browser" app | Adoption | Open source, local-only URL inspection, clear privacy statement, menu bar log of what was intercepted |
| Breaking normal browsing (slow or failed forwarding) | Very bad UX, uninstall | Forward non-music URLs immediately without network calls. Thorough testing. "Disable openstream" toggle |
| API access changes (Spotify especially) | Resolution breaks | Several layered resolvers, ISRC cache, Odesli fallback, metadata scraping fallback |
| Permissions look scary: openstream is the default link handler **and** (with autoplay) has Accessibility | Trust, installs | Autoplay is opt-in, so without it openstream never asks for Accessibility. AX is used only on Tidal/Music right after a converted link. Open source; links handled locally. Ship signed with Developer ID and notarized (like Discord/Zoom), not via the App Store (sandboxed apps can't use Accessibility) |
| Autoplay depends on the target apps' UI (CSS class names, identifiers) | Autoplay breaks after app updates | Prefix matching, verification step, debug dump to fix quickly; core conversion keeps working without it |
| Apple Music API cost ($99/yr developer program) | Small cost | Needed anyway for signing and notarization |
| Odesli rate limits / ToS | MVP scaling | Request an API key, move to direct APIs in v1 |
| Wrong-version matches | Annoyance | Score candidates on ISRC → UPC → duration. Let users report mismatches |

---

## 3. Architecture (macOS)

**Stack (decided: option A):** a **Rust core** with a **thin Swift/SwiftUI shell**, connected through [UniFFI](https://mozilla.github.io/uniffi-rs/). The shell is a menu bar app (`LSUIElement = YES`, no Dock icon) for macOS 13+.

- **Rust does** everything that isn't macOS-specific: parsing links, expanding short links, resolving and matching tracks across services, caching, and the HTTP calls. The same core can be reused on iOS and Android later (Phase 5).
- **Swift does** everything that needs macOS: receiving clicked links from the system, forwarding other links to the real browser, checking which apps are installed and opening them (`NSWorkspace`), the menu bar UI, notifications and launch at login.
- **Fast path rule:** deciding whether a link is a music link is a pure, synchronous Rust function with no network and no async. Ordinary links are forwarded without waiting on anything.

**Repo layout**

```
openstream/
├── Cargo.toml                    – Cargo workspace
├── crates/
│   ├── openstream-core/          – link parser, resolvers, matching, cache (pure Rust library)
│   ├── openstream-cli/           – `openstream parse|resolve <url>`; dev/test tool, Phase 0 helper
│   └── openstream-ffi/           – UniFFI bindings exposing core to Swift (Phase 1)
├── macos/                        – Xcode project: Swift menu bar shell (Phase 1)
└── docs/tickets/                 – work tickets
```

**Components**

```
Rust: openstream-core
├── link        – SpotifyLink parser: type + id, locale/query cleanup, `is_music_link()` (sync, no I/O)
├── expand      – short link expansion (spotify.link, spoti.fi)
├── resolve     – Resolver trait
│     ├── CacheResolver (SQLite)
│     ├── ItunesResolver (Apple Music, fuzzy match)       [v0.1]
│     ├── DeezerIsrc + TidalApi (exact ISRC/UPC match)  [v0.1]
│     ├── DirectApiResolver (Spotify → ISRC/UPC → Apple Music / Tidal)   [v1]
│     └── SearchFallbackResolver
└── target      – target service URL/deep-link builders (music://, tidal://, web fallback)

Swift: openstream.app
├── App / MenuBar UI (SwiftUI)    – status, recent conversions, pause toggle, settings
├── URLRouter                     – receives every http(s)/spotify: URL (kAEGetURL / onOpenURL)
├── BrowserForwarder              – passes non-music URLs to the user's chosen real browser
├── AppOpener                     – detects installed native apps and opens them (native first, web last)
└── Settings                      – preferred service, real browser, storefront, fallback behaviour
```

**Menu bar presence (requirement)**

When openstream is opened, it **adds an icon to the macOS top menu bar** (the status area next to Wi-Fi, battery and so on). The menu bar is the app's only visible surface. There's no Dock icon and no main window.

- Built with SwiftUI `MenuBarExtra` (macOS 13+) and `LSUIElement = YES`, so the app has no Dock icon and doesn't show in the Cmd-Tab app switcher.
- The icon appears **as soon as the app launches**, whether started by hand, by the first-run onboarding or by launch at login. It stays until the user quits from the menu.
- Icon: a monochrome template image, so macOS tints it correctly in light and dark mode. When openstream is disabled, the icon shows a crossed-out state.
- Clicking the icon opens the menu:
  - status line: "Active: Spotify → Tidal" or "Disabled: links open as usual"
  - **Disable / Enable openstream**: while disabled, Spotify links open as usual (not converted)
  - **Recent conversions** (last 10). Clicking one opens it again, with an "Open original link" option
  - **Preferred service**: Apple Music / Tidal
  - **Real browser**: the browser that ordinary links are forwarded to
  - **Launch at login** toggle
  - **Settings…**, **About**, **Quit openstream**
- Launching the app again while it's already running doesn't add a second icon.
- If openstream isn't the default browser yet, the menu shows a warning item ("Not active: set openstream as default browser") that leads to the onboarding.

**Key Info.plist entries**
- `CFBundleURLTypes`: `http`, `https`, `spotify`
- `CFBundleDocumentTypes`: `public.html` / `public.url` (required to qualify as a browser in System Settings)
- `LSUIElement`: `YES`

**Optional backend** (v1, tiny serverless function): holds the Spotify, Apple Music and Tidal credentials, signs the Apple Music developer token, and caches resolutions across users. Keeps secrets out of the app binary.

**Performance targets**
- Non-music URL forwarded in under 50 ms (no network calls).
- Music link opened in the target app in under 1.5 s on a cache miss, under 200 ms on a cache hit.

---

## 4. Development Plan

### Phase 0 — Validate (½–1 day)
- [ ] Install Finicky and write a config that sends `open.spotify.com` URLs through Odesli and opens the Apple Music/Tidal result.
- [ ] Use it for a week with real shared links. Record the match rate, failures and which apps don't go through the default browser.
- [ ] Confirm the Tidal desktop URL scheme formats and the Apple Music open behavior.

**Exit criteria:** ≥ 90% of shared track links open the correct track, and the flow feels useful.

### Phase 1 — MVP menu bar app (1–2 weeks)
- [ ] Xcode project, menu bar app shell, Info.plist URL/document types.
- [ ] Menu bar icon appears in the top menu bar on launch (`MenuBarExtra`, no Dock icon). See "Menu bar presence" in §3.
- [ ] Onboarding: pick the preferred service and the real browser, then set openstream as the default browser.
- [ ] `URLRouter` + `BrowserForwarder` (pass-through must be solid before anything else).
- [ ] `SpotifyLinkParser` with unit tests for every URL format in §2.3, including short links.
- [ ] `OdesliResolver` + `CacheResolver`.
- [ ] `AppleMusicOpener` + `TidalOpener` that **always open the native desktop app when it's installed**. Detect installed apps with LaunchServices. Use the web player only when the app is missing.
- [ ] Test the native app opening in all states: app running, app closed (cold start), app not installed.
- [ ] Fallback when nothing matches: open a search in the target app, plus a notification.
- [ ] Menu: pause toggle, last 10 conversions, "open original link instead".

**Deliverable:** a signed build that works for tracks and albums.

### Phase 2 — Robust resolution (1–2 weeks)
- [ ] `DirectAPIResolver`: Spotify → ISRC/UPC → Apple Music catalog + TIDAL API.
- [ ] Small backend for credentials and token signing.
- [ ] Candidate scoring (ISRC, UPC, duration, explicit flag, album name).
- [ ] Storefront/region detection.
- [ ] Artist link support.
- [ ] "Report wrong match" action that logs to a local file or sends anonymously, opt-in only.

### Phase 3 — Polish & distribution (1 week)
- [ ] Developer ID signing + notarization, Sparkle auto-updates, DMG.
- [ ] Launch at login (`SMAppService`).
- [ ] Custom menu bar icon replacing the SF Symbol placeholders: a monochrome template image (macOS tints it for light/dark menu bars) in three states: active, not set up, and disabled (currently `music.note`, `exclamationmark.circle`, `nosign`). Plus a matching app icon.
- [ ] Privacy page, open-source repo, README with GIF.
- [ ] Optional: Homebrew cask.

### Phase 4 — Extensions (later)
- [ ] Reverse direction and any-to-any (Apple Music ↔ Tidal ↔ Spotify ↔ YouTube Music ↔ Deezer).
- [ ] Playlist import into the user's library (OAuth with the target service).
- [ ] Safari/Chrome extension for links clicked inside the browser.
- [ ] Share menu / Services "Convert music link" for sending links back to Spotify friends.

### Phase 5 — Mobile exploration
- **iOS:** only approved browsers can become the default (needs the `com.apple.developer.web-browser` entitlement), and `open.spotify.com` universal links belong to Spotify. Realistic options are a **Share Extension** ("Open in Apple Music/Tidal"), a Shortcuts action, and a clipboard-triggered app flow. You can't get the same seamless interception as on macOS.
- **Android:** much more open. An app can declare intent filters for `open.spotify.com`. Since Android 12, unverified links go to the browser by default, but the user can enable "Open supported links" for the app in settings. Default-browser routing is also possible. Most likely to match the macOS experience.

---

## 5. Open Questions

1. Should playback **start** automatically, or should the app only show the item? This affects whether AppleScript or MusicKit is needed for Apple Music.
2. Use Odesli long-term (simple, third party), or go direct-API from v1 (more control, more credentials to manage)?
3. Open source from day one? It helps a lot with trust for a default-browser app.
4. Free, or a small paid utility? This affects backend costs and the need for an Odesli API key.
5. Which "real browser" setup should be supported: one browser, or Velja-style rules?
6. **Final name.** "openstream" is a working title. Openstream, Inc. (Openstream.ai) holds OPENSTREAM trademark filings, so it's risky for a store release. Current candidate: **Gunbu**, from *yartsa gunbu*, the Tibetan name for the caterpillar fungus. No app, brand or software of that name was found in a web and App Store search (2026-10-22). Watch out: searches for "gunbu" return gun software, and the fungus is a parasite, which is an awkward story for a default-browser app. Still to check: trademarks, domains, exact App Store and Play Store names.

---

## 6. Success Metrics

- **Match accuracy:** ≥ 95% of shared track links open the correct recording.
- **Native-first:** 100% of conversions open in the native app when it's installed (0 cases of the web player when the app exists).
- **Latency:** p95 < 1.5 s from click to target app showing the track.
- **Transparency:** 0 reported cases of a normal (non-music) link being blocked or noticeably slowed.
- **Retention:** user keeps openstream as the default browser after 30 days.
