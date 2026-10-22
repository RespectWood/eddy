# 0001 — Rust workspace + Spotify link parser + CLI

| | |
|---|---|
| **Status** | Open |
| **Phase** | 0 → 1 (foundation) |
| **Estimate** | 1–2 days |
| **Depends on** | — |
| **Blocks** | Resolver (Odesli), UniFFI bindings, Swift shell |

## Why

Everything in openstream starts with one question: "Is this a music link, and what does it point to?" It runs on **every link the user clicks**, so it has to be correct and instant, with no network calls. It's also the easiest part to build without any macOS or API work, which makes it the right first job. It sets up the Cargo workspace, CI and conventions that later tickets build on.

## Scope

### 1. Cargo workspace
- Root `Cargo.toml` workspace with `resolver = "2"`, a shared `[workspace.package]` (edition 2021, license, repository) and shared lints.
- `rust-toolchain.toml` pinned to `stable`, with `rustfmt` and `clippy`.
- Crates:
  - `crates/openstream-core` (library)
  - `crates/openstream-cli` (binary named `openstream`)

### 2. `openstream-core::link`
Public API (names can change in review):

```rust
pub enum SpotifyKind { Track, Album, Artist, Playlist, Episode, Show }

pub struct SpotifyLink {
    pub kind: SpotifyKind,
    pub id: String,           // base62, 22 chars
}

pub enum ParsedLink {
    Spotify(SpotifyLink),
    SpotifyShortLink(Url),    // needs network expansion (later ticket)
    NotMusic,
}

/// Sync, no I/O, no allocation-heavy work. Called for every clicked URL.
pub fn parse(url: &str) -> ParsedLink;

/// Convenience for the fast path in the Swift shell.
pub fn is_music_link(url: &str) -> bool;

impl SpotifyLink {
    pub fn canonical_url(&self) -> String;   // https://open.spotify.com/{kind}/{id}
    pub fn uri(&self) -> String;             // spotify:{kind}:{id}
}
```

Must handle:
- [ ] `https://open.spotify.com/track/{id}`
- [ ] Query and fragment removed: `?si=…`, `?context=…`, `#…`
- [ ] Locale prefix: `https://open.spotify.com/intl-de/track/{id}` (any `intl-xx` / `intl-xx-YY`)
- [ ] Every kind: track, album, artist, playlist, episode, show
- [ ] `http://`, a trailing slash, uppercase host
- [ ] URI scheme: `spotify:track:{id}`, and the older `spotify:user:{user}:playlist:{id}`
- [ ] `play.spotify.com/...` (old host)
- [ ] Short links `spotify.link/*` and `spoti.fi/*` → `SpotifyShortLink` (no network yet)
- [ ] Everything else → `NotMusic`: other hosts, `open.spotify.com/` root, `/user/…`, `/embed/…` pages to decide, invalid IDs, look-alike hosts such as `open.spotify.com.evil.tld`

### 3. `openstream-cli`
```
openstream parse <url>     # prints the parsed result as JSON
```
Used for testing by hand now. Later it gets `resolve`, which can be called from a Finicky script during Phase 0.

### 4. Tests
- Table-driven unit tests that cover every case above.
- A `tests/fixtures/links.txt` corpus of real shared links (with `si=` tokens removed) and the expected results.
- A benchmark or test assertion that `parse()` takes well under 1 ms. The target is a few µs.

### 5. CI
- `.github/workflows/ci.yml` running on `macos-latest`: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.

## Out of scope
- Network calls: short link expansion and resolving (ticket 0002: Odesli resolver).
- UniFFI bindings and Swift (ticket 0003).
- Apple Music / Tidal link parsing (later, for the reverse direction).

## Acceptance criteria
- [ ] `cargo build`, `cargo test`, `cargo clippy -- -D warnings` and `cargo fmt --check` pass locally and in CI.
- [ ] `openstream parse "https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=abc"` prints `{"type":"spotify","kind":"track","id":"4uLU6hMCjMI75M1A2tKUQC"}`.
- [ ] Every input format listed above has at least one test.
- [ ] `openstream-core` has no network or async dependencies. Check with `cargo tree -p openstream-core`.

## Setup notes
- Rust isn't installed on this machine yet. Install it with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, or `brew install rustup` followed by `rustup-init`.
- Suggested crates: `url` (parsing) and `serde` + `serde_json` (CLI output). Keep `openstream-core` dependencies to a minimum.

## Next tickets (draft)
- **0002:** Odesli resolver + short link expansion + `openstream resolve <url>`
- **0003:** `openstream-ffi` (UniFFI) + Swift package build script
- **0004:** Swift menu bar shell: receive links, forward to real browser, open native app
