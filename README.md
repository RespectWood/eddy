# openstream

**Click any Spotify link, anywhere on your Mac, and it opens in your app.**

openstream is a small macOS menu bar app for people who use Apple Music or Tidal. When a friend sends you a Spotify link, openstream finds the same track and opens it in your streaming app's desktop app instead of Spotify's web player. You don't have to search for it by hand. All other links go straight to your normal browser.

> Status: early development. Nothing to install yet.

## How it works

openstream registers as the macOS default browser and checks each clicked link locally:

- **Spotify link** → find the same track on your service (by ISRC) → open it in the native Apple Music or Tidal app
- **Any other link** → passed straight to your real browser, with no network calls

## Architecture

- `crates/openstream-core`: Rust library that parses links and resolves them across services
- `crates/openstream-cli`: command-line tool for development and testing
- `macos/`: thin Swift/SwiftUI menu bar app, connected to the core through UniFFI

See [PLAN.md](PLAN.md) for the full analysis, market scan and roadmap, and [docs/tickets/](docs/tickets/) for the current work.

## Development

Requirements: macOS 13+, the [Rust toolchain](https://rustup.rs) (stable), and Xcode 15+ (only needed for the app shell).

```sh
cargo build
cargo test
```
