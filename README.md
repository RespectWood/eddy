# Eddy

**Click any Spotify link, anywhere on your Mac, and it opens in your preferred streaming app.**

Eddy is a small macOS menu bar app for people who use Apple Music or Tidal. When a friend sends you a Spotify link, Eddy finds the same track and opens it in your streaming app's desktop app instead of Spotify's web player. You don't have to search for it by hand. All other links go straight to your normal browser.

> Status: early development. Nothing to install yet.

## How it works

Eddy registers as the macOS default browser and checks each clicked link locally:

- **Spotify link** → find the same track on your service (by title, artist and duration, or exactly by ISRC for Tidal) → open it in the native Apple Music or Tidal app
- **Any other link** → passed straight to your real browser, with no network calls

## Architecture

- `crates/eddy-core`: Rust library that parses links and resolves them across services
- `crates/eddy-cli`: command-line tool for development and testing
- `crates/eddy-ffi`: UniFFI bindings for Swift
- `macos/`: thin Swift/SwiftUI menu bar app (SwiftPM package), connected to the core through UniFFI

See [PLAN.md](PLAN.md) for the full analysis, market scan and roadmap, and [docs/tickets/](docs/tickets/) for the current work.

## Development

Requirements: macOS 13+, the [Rust toolchain](https://rustup.rs) (stable), and the Xcode Command Line Tools (full Xcode not required).

```sh
cargo test                                  # unit tests (no network)
cargo test -- --ignored                     # live tests against Spotify, iTunes, Deezer
scripts/build-app.sh                        # → dist/Eddy.app + zip (universal, ad-hoc signed)
ARCHS=arm64 scripts/build-app.sh            # faster, Apple silicon only
```

Try the core without the app:

```sh
cargo run -p eddy-cli -- parse "https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=x"
cargo run -p eddy-cli -- resolve "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC" --to apple-music --country DE
```

For exact Tidal matches, create a free app at [developer.tidal.com](https://developer.tidal.com) and enter its client ID/secret in the app's settings (or set `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET` for the CLI). Without them, Tidal opens a search instead.

Before a release, run the [manual tests](docs/manual-tests.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in this work, as defined in the Apache-2.0 license, will be dual licensed as above, without any additional terms or conditions.
