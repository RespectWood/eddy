# openstream mobile — Rundown, Analysis & Development Plan

> Status: idea / planning · Platforms: iOS, Android · Builds on: [PLAN.md](PLAN.md) (macOS) · Date: 2026-10-22

---

## 1. Rundown

**Problem.** Same as on the Mac, but where most links are actually received: on the phone. A friend sends a Spotify link in Messages, WhatsApp, Signal, Telegram or Discord. The recipient uses Apple Music or Tidal. Tapping the link opens Spotify's web page (or the App Store page for Spotify), and they have to search for the song by hand.

**Desired result.** Tapping a Spotify link on the phone opens the same track in the **native Apple Music or Tidal app**, with as few extra taps as the platform allows.

**One-liner.** *openstream for your phone: Spotify links open in your music app.*

**The big difference from macOS.** On the Mac, openstream can sit between every link click and the browser. Phones are more locked down:

- **Android** can come close to the macOS experience (intercept links system-wide).
- **iOS** cannot. Apple does not let a third-party app take over `open.spotify.com` links or act as a link router. On iOS the realistic goal is **"one or two taps"** (Safari extension, share sheet, paste), not "zero taps".

The plan has to be honest about this. iOS is not a port of the Mac app. It's a different product built on the same Rust core.

---

## 2. Analysis

### 2.1 What happens when a link is tapped

```
tap link ──► [1] how does the OS route it? ──► [2] parse ──► [3] resolve ──► [4] open target app
                    │
                    ├── Spotify app installed  ──► Spotify opens (universal link / verified app link)
                    ├── in-app browser (Instagram, Slack, …) ──► stays inside that app
                    └── default browser ──► Safari / Chrome / …
```

Steps 2–4 are the same as on macOS and reuse `openstream-core`. Step 1 is where mobile differs: openstream has to get the link from one of the routes above, and some routes it can never reach.

**Important assumption:** most target users (people who use Apple Music or Tidal) **don't have the Spotify app installed**. That matters because if Spotify is installed, it owns its links on both platforms and openstream mostly can't get in between.

### 2.2 Interception options on iOS

| Approach | How it works | Coverage | Effort | Verdict |
|---|---|---|---|---|
| **A. Safari Web Extension** | Extension watches navigations to `open.spotify.com` and redirects to the converted Apple Music/Tidal link | Taps in Messages, WhatsApp, Mail etc. **when Safari is the default browser and Spotify isn't installed**. That's probably most of the target group. | Medium | ✅ **Primary (automatic) path** |
| **B. Share Extension** | User long-presses the link → Share → "openstream" → target app opens | Works from almost any app, any browser, even with Spotify installed | Low–Medium | ✅ **Primary (manual) path** |
| **C. In-app paste** | Open openstream, tap a paste button (`UIPasteControl`, no permission prompt) → opens target | Everything, but manual | Low | ✅ Fallback, also used for onboarding/testing |
| **D. App Intent / Shortcuts action** | "Open in my music app" action. Users can wire it into Shortcuts or the share sheet | Power users | Low | ➕ Add-on, cheap with App Intents |
| E. Become the default browser | Needs the `com.apple.developer.web-browser` entitlement. Apple only grants it to real browsers (URL field, renders any web page, etc.) | Would be system-wide | Very high | ❌ Rejected: openstream would have to be a full browser, and forwarding other links to the real browser is not something this entitlement is meant for. High App Review risk. |
| F. Claim `open.spotify.com` universal links | Universal links are tied to the domain owner's `apple-app-site-association` file | — | — | ❌ Impossible: only Spotify can do this |
| G. Claim the `spotify:` URL scheme | If two apps declare the same scheme, iOS picks one and doesn't say which | Very low, shared links are `https://` | Low | ❌ Not worth it, and unreliable |

**Notes on A (Safari extension):**
- Safari Web Extensions on iOS are JavaScript + a native handler. The JS can call native code with `browser.runtime.sendNativeMessage`, so the native handler can run the Rust core. No server needed.
- The user has to enable the extension in **Settings → Apps → Safari → Extensions** and grant access to `open.spotify.com`. Onboarding has to walk them through this; it's the main drop-off point.
- **Needs testing:** what happens after the redirect. A script redirect to `https://music.apple.com/...` or `https://tidal.com/...` may stay in Safari instead of opening the app (universal links are usually only followed on a user tap). A custom scheme (`music://`, `tidal://`) opens the app but Safari asks "Open in Tidal?" first. Option: show a small openstream page with one big "Open in Tidal" button, which counts as a user tap.
- Doesn't help users whose default browser is Chrome, Firefox, Arc etc. (Chrome on iOS has no extensions.) For them: share extension.

**Notes on B (Share Extension):**
- Extensions can't call `UIApplication.shared.open`, so a share extension can't open another app directly in the normal way. **Needs testing:** options are an **Action Extension**, handing off to the main openstream app (which then opens the target), or showing the result with a button. Pick whichever is fewest taps and passes App Review.
- Settings and the cache must be shared between the app and its extensions through an **App Group** container.

### 2.3 Interception options on Android

| Approach | How it works | Coverage | Effort | Verdict |
|---|---|---|---|---|
| **A. App links / intent filter for `open.spotify.com`** | Declare an `https` intent filter for `open.spotify.com` (and `spotify.link`, `spoti.fi`) | Every app that opens links with a normal `VIEW` intent | Low | ✅ **Primary path** |
| **B. Default browser role** (`RoleManager.ROLE_BROWSER`) | Like macOS: become the browser, handle Spotify links, forward everything else to the real browser with an explicit intent | Everything that goes to the browser, without per-domain setup | Medium | ➕ Alternative mode if A's setup turns out too confusing |
| **C. Share target** (`ACTION_SEND`, `text/plain`) | Share a link to openstream | Everything, manual | Low | ✅ Fallback |
| D. Clipboard watcher | Background clipboard access is blocked since Android 10 | — | — | ❌ |

**Notes on A:**
- Since **Android 12**, links for domains the app hasn't verified don't open in it by default. openstream can't verify `open.spotify.com` (it isn't ours), so the user has to turn it on: **Settings → Apps → openstream → Open by default → Add links**. The app can deep-link straight to that screen (`Settings.ACTION_APP_OPEN_BY_DEFAULT_SETTINGS`) and check the state with `DomainVerificationManager`.
- **If the Spotify app is installed**, it is verified for its own domain and wins. The user would need to turn off "Open supported links" for Spotify. Needs testing on Android 12–15.
- In-app browsers and Custom Tabs inside apps (Instagram, some Slack/Discord settings) won't hand the link over.

**Why Android is closer to macOS:** after a one-time setting, taps in WhatsApp, Telegram, Messages, Gmail and Chrome go straight to openstream with no extra taps. Non-Spotify links are never touched in mode A, so the "trust" problem from macOS (seeing every link) mostly disappears.

### 2.4 Resolving: same core, two mobile-specific changes

`openstream-core` (link parsing, short-link expansion, iTunes/Deezer/Tidal resolvers, release ranking from ticket 0002, SQLite cache) is reused unchanged. Things to check or change:

- **Blocking HTTP.** The core uses `ureq` (blocking, rustls). That's fine on mobile as long as the shells call it off the main thread (Swift `Task.detached`, Kotlin `Dispatchers.IO`). UniFFI async support is an option later.
- **Cache location.** `Cache::open(path)` already takes a path, so the shells pass the App Group container (iOS) or `filesDir` (Android).
- **Tidal credentials.** On macOS the user enters their own Tidal client ID/secret. Asking phone users to create a TIDAL developer app is unrealistic, and shipping the secret inside the app binary isn't safe. This pushes the **small backend** from PLAN.md Phase 2 earlier for mobile, or Tidal stays search-only on mobile until the backend exists.
- **Apple Music on iOS: MusicKit.** On iOS, MusicKit for Swift creates developer tokens automatically. That gives exact **ISRC lookups** in the Apple Music catalog (instead of the fuzzy iTunes Search match) without a backend. It can also **start playback** (`SystemMusicPlayer`) if the user grants Apple Music access, which answers PLAN.md open question 1 on iOS. Needs checking: whether catalog requests need the user's permission, and how MusicKit behaves without a subscription.
- **Storefront.** iOS: `MusicKit` / `SKStorefront` gives the user's real storefront. Android: device locale/region, with a setting to override.

### 2.5 Opening the target app

> **Same hard requirement as macOS: if the user's streaming app is installed, open the native app, never the web player.**

**iOS**
- Apple Music: open `https://music.apple.com/{storefront}/album/...?i={trackId}` with `UIApplication.open(_:options: [.universalLinksOnly: true])`. This only succeeds if the Music app handles it. If it fails, fall back to the web page and tell the user.
- Tidal: universal link `https://tidal.com/browse/track/{id}` with `.universalLinksOnly`, or the `tidal://` scheme. Checking with `canOpenURL("tidal://")` requires `tidal` in `LSApplicationQueriesSchemes`. Formats need testing on device.
- Apple Music playback via MusicKit (optional, see §2.4).

**Android**
- Send an explicit `ACTION_VIEW` intent with the target package set, so it can't bounce back to openstream or open in a browser:
  - Apple Music: package `com.apple.android.music` (to confirm), `https://music.apple.com/...` URL.
  - Tidal: package `com.aspiro.tidal` (to confirm), `https://tidal.com/browse/track/{id}` or `tidal://track/{id}`.
- Check installed apps with `PackageManager`. Since Android 11 this needs a `<queries>` block in the manifest listing those packages.
- Not installed → open the web URL in the real browser (explicit intent, never back to openstream) and offer a Play Store link.

### 2.6 Out of scope for mobile MVP

Same as macOS: playlists, podcasts (pass through), Spotify exclusives (fallback UI), reverse direction. Also out of scope:
- **iPad-specific UI.** The iPhone app runs on iPad. A dedicated layout can come later.
- **Widgets, Live Activities, watchOS, Wear OS.**
- **Becoming a browser on iOS** (§2.2 E).

### 2.7 Market (mobile)

From the macOS market scan in [PLAN.md §2.7](PLAN.md): the existing automatic redirectors (**Convusic**, **Euphony**, **MusicMatch**) are all **iOS Safari extensions**, so on iOS openstream is *not* unique. They are direct competitors on the same mechanism.

| | iOS | Android |
|---|---|---|
| Existing receiver-side redirectors | Convusic, Euphony, MusicMatch (Safari extension + share) | None found in the macOS scan. **Needs its own Play Store scan.** |
| openstream's angle | Better matching (original release, not compilations; ticket 0002), first-class Tidal, open source, same app on Mac | Possibly first real system-level redirector. Strongest mobile opportunity. |

**USP options:**
- **iOS:** "The same openstream you use on your Mac, on your iPhone." Quality of matching + Tidal support + open source. Not "the only one".
- **Android:** "Tap any Spotify link and it opens in Apple Music or Tidal." Same pitch as macOS, if the Play Store scan confirms the gap.

**Validation before building:**
- [ ] Scan the Play Store for Spotify → Apple Music/Tidal link redirectors.
- [ ] Install Convusic and Euphony on iOS. Check match quality (compilations?), Tidal behavior, and how many taps they need.
- [ ] Ask in r/TIdaL, r/AppleMusic, r/androidapps: do people want this on the phone more than on the Mac?

### 2.8 Risks

| Risk | Impact | Mitigation |
|---|---|---|
| iOS users don't enable the Safari extension | Automatic path unused | Clear onboarding with screenshots, test it from inside the app, share extension always works |
| Spotify app installed → owns its links | No automatic interception | Detect it during onboarding and explain the options (Android: disable Spotify's supported links; iOS: share sheet) |
| Android "Open by default" setup confuses users | Adoption | Deep link to the exact settings screen, check the state and show "Active ✓" / "Not active" |
| App Review rejects iOS app (link redirect, extension behavior) | Launch blocked | Keep it simple and honest: no browser entitlement, no private APIs, no responder-chain hacks to open apps |
| Tidal credentials can't ship in the app | Tidal falls back to search | Backend proxy (PLAN.md Phase 2) before Tidal exact match on mobile |
| Fragmented Android behavior (Samsung, Xiaomi, Android versions) | Bugs | Test matrix in the manual tests: Pixel + Samsung, Android 12 and latest |
| Two more platforms to maintain | Effort | Keep shells thin; all logic and tests stay in Rust |

---

## 3. Architecture (mobile)

**Stack:** the same **Rust core** + **thin native shells**, connected through UniFFI. UniFFI already generates Swift bindings for macOS. It generates Kotlin bindings from the same `openstream-ffi` crate.

- **iOS:** Swift/SwiftUI app (iOS 17+) + Safari Web Extension + Share/Action Extension, sharing an App Group. Rust is shipped as an **XCFramework** (`aarch64-apple-ios`, `aarch64-apple-ios-sim`).
- **Android:** Kotlin + Jetpack Compose app (Android 10+, API 29). Rust is built with **cargo-ndk** for `arm64-v8a`, `armeabi-v7a`, `x86_64` (emulator) and loaded through JNA (UniFFI's Kotlin default).

**Repo layout (added)**

```
openstream/
├── crates/
│   └── openstream-ffi/        – also generates Kotlin bindings
├── macos/                     – existing
├── ios/                       – Xcode project: app, Safari extension, share extension
├── android/                   – Gradle project: app module + generated bindings
└── scripts/
    ├── build-ios.sh           – Rust → XCFramework + Swift bindings
    └── build-android.sh       – Rust → jniLibs via cargo-ndk + Kotlin bindings
```

**Components**

```
Rust: openstream-core / openstream-ffi       – unchanged; shared by macOS, iOS, Android

Swift: openstream iOS
├── App (SwiftUI)                 – onboarding, paste button, recent conversions, settings
├── SafariExtension               – JS: catch open.spotify.com → native handler → Rust → redirect / "Open in" page
├── ShareExtension                – receive URL → Rust → hand off / open target
├── AppOpener                     – universal links first, web fallback
├── MusicKitResolver (optional)   – exact ISRC lookup + playback for Apple Music
└── Shared (App Group)            – settings + SQLite cache

Kotlin: openstream Android
├── LinkActivity                  – receives VIEW intents for open.spotify.com / spotify.link / spoti.fi, no UI if it can help it
├── ShareActivity                 – receives ACTION_SEND
├── AppOpener                     – explicit intents to Apple Music / Tidal packages, web fallback
├── Onboarding                    – pick service, enable "Open by default", show status
└── Settings / History (Compose)
```

**Android "invisible" handling:** `LinkActivity` uses a translucent theme, resolves on a background thread, opens the target and finishes. The user should only see a short spinner/toast, never a full app screen.

**Performance targets**
- Tap → target app showing the track: p95 < 2 s on a cache miss (mobile networks are slower than desktop), < 300 ms on a cache hit.
- Android `LinkActivity` visible for < 1 s on a cache hit.

---

## 4. Development Plan

**Recommendation: iOS first, Android right after.** Apple Music users mostly carry iPhones, and the Swift + UniFFI pipeline already exists from macOS. Android is the stronger product, though, so if the Play Store scan (§2.7) shows an empty market, consider swapping the order. See open question 1.

### Phase M0 — Validate (2–3 days)
- [ ] Cross-compile `openstream-core` for iOS and Android targets and run the existing tests on a simulator/emulator (catches rustls, SQLite or path issues early).
- [ ] On real phones, tap Spotify links from Messages, WhatsApp, Signal, Telegram, Discord, Slack, Instagram and Mail. Record what opens: **with and without** the Spotify app installed, Safari vs. Chrome as default (iOS), Pixel vs. Samsung (Android).
- [ ] Confirm Apple Music and Tidal deep link / universal link formats and package names on both platforms, in all states: app running, closed, not installed.
- [ ] Prototype: does a Safari extension redirect to `music.apple.com` / `tidal.com` open the app, stay in Safari, or show a prompt?
- [ ] Play Store competitor scan; try Convusic/Euphony on iOS.

**Exit criteria:** a clear answer to "how many taps does iOS need?" and "does Android intercept reliably after setup?"

### Phase M1 — iOS MVP (2–3 weeks)
- [ ] `scripts/build-ios.sh` → XCFramework + Swift bindings.
- [ ] Xcode project with App Group shared by app and extensions.
- [ ] Onboarding: pick preferred service, enable the Safari extension (with screenshots), test button.
- [ ] Paste button flow (`UIPasteControl`) → resolve → open native app.
- [ ] Share/Action extension for Spotify links.
- [ ] Safari Web Extension for `open.spotify.com`, `spotify.link`, `spoti.fi`, using the native handler.
- [ ] `AppOpener` with universal links only + web fallback + "install the app" hint.
- [ ] Recent conversions + "open original link instead".
- [ ] Apple Music via existing iTunes resolver; Tidal via search fallback (until the backend exists).

**Deliverable:** TestFlight build that handles tracks and albums.

### Phase M2 — Android MVP (2 weeks)
- [ ] `scripts/build-android.sh` (cargo-ndk) + Kotlin bindings in the Gradle build.
- [ ] Intent filters for Spotify domains; `LinkActivity` + `ShareActivity`.
- [ ] Onboarding: pick service, deep link to "Open by default", live status check via `DomainVerificationManager`.
- [ ] `AppOpener` with explicit package intents, `<queries>` manifest block, web fallback.
- [ ] Recent conversions + settings (Compose).

**Deliverable:** signed APK + Play Store internal testing track.

### Phase M3 — Better matching on mobile (1–2 weeks)
- [ ] Backend proxy for Tidal (and Spotify if needed) credentials — shared with PLAN.md Phase 2.
- [ ] iOS: `MusicKitResolver` for exact ISRC matches; optional "start playing" setting.
- [ ] Storefront detection (iOS MusicKit, Android region + override).
- [ ] Cache shared correctly between iOS app and extensions.

### Phase M4 — Polish & release (1 week)
- [ ] App Store and Play Store listings, privacy labels / data safety form (only music links leave the device).
- [ ] Manual test plan for mobile in [docs/manual-tests.md](docs/manual-tests.md) (device/app matrix from M0).
- [ ] Android: optional default-browser mode (§2.3 B) if the "Open by default" setup is too much friction.
- [ ] iOS: App Intents / Shortcuts action.

### Later
- Reverse direction / any-to-any, playlist import, iPad layout, widgets — same list as PLAN.md Phase 4.

---

## 5. Open Questions

1. **iOS or Android first?** iOS fits the audience; Android fits the product. Depends on the M0 and Play Store results.
2. Is "one or two taps" on iOS good enough to be worth shipping, given that Convusic and Euphony already exist there?
3. Should the mobile apps be **paid** (App Store developer account is needed anyway; Play Store one-time fee), free, or open source with paid store builds?
4. Should Apple Music on iOS **start playing** via MusicKit, or only show the item like macOS? Asking for Apple Music access adds a permission prompt.
5. When does the backend go live? Without it, Tidal on mobile is search-only.
6. Should settings and history sync between Mac and phone (iCloud key-value store), or stay per device?

---

## 6. Success Metrics

- **Match accuracy:** ≥ 95% of shared track links open the correct recording (same as macOS, same core).
- **Native-first:** 100% of conversions open the native app when it's installed.
- **Taps:** Android 0 extra taps after setup; iOS ≤ 1 extra tap via Safari extension, ≤ 2 via share sheet.
- **Setup completion:** ≥ 70% of new users finish enabling the Safari extension (iOS) or "Open by default" (Android).
- **Latency:** p95 < 2 s from tap to the target app showing the track.
- **Retention:** user still has openstream active after 30 days.
