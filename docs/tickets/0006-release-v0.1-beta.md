# 0006 — Release v0.1.0-beta.1

| | |
|---|---|
| **Status** | Open |
| **Phase** | 1 (closes it) |
| **Estimate** | ½–1 day, plus fixes for whatever the manual tests turn up |
| **Depends on** | 0001–0004, 0009, 0010 |

Planned as `v0.1.0-alpha`; renamed to **beta** (2026-10-24) because the features for 0.1 are complete and the release goes to outside testers (~50). `0.1.0-alpha` was never tagged, so `v0.1.0-beta.1` is the first release. Fixes from the test round ship as `beta.2`, `beta.3`, …; **1.0** waits for Developer ID signing and notarization, a settled TIDAL situation, and the testers' feedback.

## Goal

Turn what's on `main` into a **tagged pre-release** that testers can install: manually tested, known limitations written down, a GitHub release with the zip and clear install notes.

## Scope

### 1. Manual test pass
- [ ] Run [docs/manual-tests.md](../manual-tests.md) on a release build (universal, `scripts/build-app.sh`) and fill in the Result column. Required:
  - **A** (launch, menu bar), **B** (setup, default link handler), **C** (ordinary links still work)
  - **E1–E11** (TIDAL, incl. right release and landing page)
  - **F1–F4** (other link types, real short link)
  - **G1–G5** (where links come from)
  - **H** (menu controls, incl. Disable/Enable)
  - **I1–I4** (launch at login, offline)
  - **K1–K5, K7** (autoplay; K6 needs an Apple Music subscription)
  - **L1–L7** (Connect TIDAL), and the new one-line TIDAL row with logo and Privacy Policy row, in light and dark mode
  - **O1–O11** (Copy Link for Current Song: TIDAL, Apple Music, Spotify; O7 decides whether the submenu prefetch works, see the debug log)
- [ ] Security checks from 0010: `open "eddy://debug/share"` without a token does nothing; a forged `eddy://auth/tidal?error=…` during a login is ignored; a TIDAL developer secret saved by an older build is gone from `defaults read com.respectwood.eddy`.
- [ ] Apple Music sections (D, K6) tested where possible, otherwise listed as known gaps (no subscription on the dev Mac).
- [ ] Every ❌ becomes either a fix in this ticket (small) or a new ticket (bigger), linked from the test row.

### 2. Housekeeping
- [x] The empty `feat:` commit (`7243fea`) is already pushed, so it stays.
- [x] Version `0.1.0-beta.1` in `Cargo.toml` (the app shows `0.1.0`, build number from the commit count).
- [x] `CHANGELOG.md`: "Unreleased" became `0.1.0-beta.1`, with a **Known limitations** list.
- [x] `CHANGELOG.md`: release date in the `0.1.0-beta.1` heading.
- [x] README: Install, Uninstall, Privacy, status "beta".
- [x] Ticket statuses: 0001–0004 and 0009 in `done/`.
- [x] Release build: debug commands stay opt-in (`debugToken`, see 0010); the build only reads the public TIDAL client ID from `.env`.

### 3. Make the repository public
- [ ] Read through the README once more (it's the landing page).
- [ ] GitHub → Settings → Security: turn on **Private vulnerability reporting** (`SECURITY.md` points there).
- [ ] Branch ruleset for `main`: block force pushes, restrict deletions.
- [ ] Switch the repository to public. The privacy link in eddy's settings (`github.com/RespectWood/eddy#privacy`) only works from then on.

### 3a. TIDAL (Developer Terms v3.0, see 0004)
- [ ] **Q5:** a second TIDAL account (a friend) presses Connect on a release build. If it fails ("Request not allowed", sub_status 1005), the release notes say exact TIDAL matches only work for the developer for now; TIDAL search still works for everyone.
- [x] README **Privacy** section, linked from Settings → Accounts → Privacy Policy, public before anyone signs in (Annex 1, 3.3).
- [x] README: not affiliated with TIDAL, Spotify or Apple; for personal use.
- [x] TIDAL branding (Developer Guidelines II, Design Guidelines): official logo linking to tidal.com in Settings → Accounts; "TIDAL" in capitals in the app.
- [x] `SECURITY.md`: a leak of TIDAL tokens is reported to `dataprotectionofficer@tidal.com` within 24 hours (Annex 1, 3.11).
- [ ] Production Mode: text ready in [docs/tidal-production-submission.md](../tidal-production-submission.md). **There's currently no way to submit it**: the dashboard has no form (tidal-music discussions #404, #179). Submit as soon as one appears.
- [x] Release without waiting for approval; the release notes explain the limits.
- Known from the discussions (2026-10-24), for later:
  - Rate limits are **per app**, shared by all users (#135); no quota increases while the API is in beta. eddy only prefetches when the Copy Link submenu opens and remembers misses for 10 minutes.
  - Production apps may need **HTTPS redirect URIs** (#255), so `eddy://auth/tidal` would have to become an HTTPS page that forwards to eddy, or the Device Code flow.
  - Fallback if the shared quota runs out: testers register their own free developer app and use its client ID (#354); needs a small change to Connect TIDAL.

### 4. Build and release
- [x] Universal build (`scripts/build-app.sh`) → `dist/eddy-0.1.0-beta.1-macos.zip` (5.4 MB, hardened runtime, signature checked after unzipping).
- [x] Tag `v0.1.0-beta.1` (on `ed1868e`), pushed.
- [x] GitHub **pre-release** with the zip; notes from [docs/releases/v0.1.0-beta.1.md](../releases/v0.1.0-beta.1.md) (published 2026-10-24; visible once the repository is public).

### 5. Optional: release automation
- [ ] A CI job on tags that builds the zip and attaches it to the release (signing stays local until there's a Developer ID).

## Out of scope

- Developer ID signing + notarization (needs the paid Apple Developer account; for 1.0).
- New features.

## Acceptance criteria

- [ ] Manual test results recorded; every failure fixed or ticketed.
- [ ] Repository public, private vulnerability reporting on.
- [x] `v0.1.0-beta.1` tag and GitHub pre-release with the zip and release notes.
- [ ] CI green on the tagged commit.
