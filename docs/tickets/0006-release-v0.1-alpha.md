# 0006 — Release v0.1.0-alpha

| | |
|---|---|
| **Status** | Open |
| **Phase** | 1 (closes it) |
| **Estimate** | ½–1 day, plus fixes for whatever the manual tests turn up |
| **Depends on** | 0001–0004, 0010 (items 1–2) |

## Goal

Turn what's on `main` into a **tagged, stable version** to use every day before new features land: everything manually tested, known issues written down, and a GitHub release you can reinstall from.

## Scope

### 1. Manual test pass
- [ ] Run [docs/manual-tests.md](../manual-tests.md) on the current build and fill in the Result column. Not every test is needed. Required for the release:
  - **A** (launch, menu bar), **B** (setup, default link handler), **C** (ordinary links still work)
  - **E1–E11** (Tidal, incl. right release and landing page)
  - **F1–F4** (other link types, real short link)
  - **G1–G5** (where links come from)
  - **H** (menu controls, incl. Disable/Enable)
  - **I1–I4** (launch at login, offline)
  - **K1–K5, K7** (autoplay; K6 needs an Apple Music subscription)
  - **L1–L7** (Connect Tidal)
- [ ] Apple Music sections (D, K6) are tested where possible and otherwise listed as known gaps (no subscription on the dev Mac).
- [ ] Every ❌ becomes either a fix in this ticket (small) or a new ticket (bigger), linked from the test row.

### 2. Housekeeping
- [ ] Reword the empty `feat:` commit (`7243fea`) if it isn't pushed yet; otherwise leave it.
- [ ] `CHANGELOG.md`: move "Unreleased" into `0.1.0-alpha` with the date; add a **Known limitations** list (Apple Music autoplay untested, Spotify links inside the browser not caught (0007), artist/playlist links not converted (0008), Tidal development-mode limits).
- [ ] README: an **Install** section (download the zip, move to Applications, first-run steps: default browser, Connect Tidal, optional Accessibility for autoplay) and a short **Uninstall** section (set the default browser back first).
- [ ] Ticket statuses: 0001–0004 updated to their final state.
- [ ] Turn off the developer settings on the release build path: debug commands stay opt-in (they need the `debugToken` default, see 0010), and the release build doesn't depend on the local `.env` except for the public Tidal client ID.

### 3. Build and release
- [ ] Universal build (`scripts/build-app.sh`).
- [ ] Tag `v0.1.0-alpha`, push the tag.
- [ ] GitHub **pre-release** with the zip and release notes from the CHANGELOG.
- [ ] Release notes say clearly: **signed with a local development certificate, so it only installs cleanly on the developer's Mac.** Others need the right-click → Open workaround, and the Accessibility permission has to be re-granted after updates. Proper Developer ID signing and notarization needs the paid Apple Developer account (PLAN.md Phase 3).

### 4. Optional: release automation
- [ ] A CI job on tags that builds the zip and attaches it to the release (signing stays local until there's a Developer ID).

## Out of scope

- Developer ID signing + notarization (needs the Apple Developer account).
- New features.

## Acceptance criteria

- [ ] Manual test results recorded; every failure fixed or ticketed.
- [ ] `v0.1.0-alpha` tag and GitHub pre-release with the zip.
- [ ] CHANGELOG and README updated (install, uninstall, known limitations).
- [ ] CI green on the tagged commit.
