//! Eddy core: turns Spotify links into links for the user's own streaming service.
//!
//! - [`link`] parses clicked URLs. Sync and free of I/O; runs on every click.
//! - [`Resolver`] finds the same item on Apple Music or Tidal. Blocking network calls.
//! - [`Resolver::share_link`] goes the other way: a Spotify, Apple Music or TIDAL link
//!   for the song playing.
//! - [`target`] builds the URLs that open items in the native apps.

#![forbid(unsafe_code)]

// Recorded responses are for tests; a release build must never answer from them.
#[cfg(all(feature = "test-support", not(debug_assertions)))]
compile_error!("the `test-support` feature is for test builds only");

mod cache;
mod deezer;
mod error;
mod expand;
mod explain;
mod http;
mod itunes;
pub mod link;
mod matching;
mod musicbrainz;
#[cfg(test)]
mod offline_tests;
#[cfg(any(test, feature = "test-support"))]
mod replay;
mod resolve;
mod reverse;
mod spotify;
pub mod target;
mod tidal;
pub mod tidal_auth;

pub use error::{NotFoundReason, ResolveError};
pub use link::{is_music_link, parse, ParsedLink, SpotifyKind, SpotifyLink};
pub use resolve::{Conversion, Resolver, ResolverConfig};
pub use reverse::{NowPlaying, ShareTarget, SongShare};
pub use target::{Service, TargetUrls};
pub use tidal::TidalCredentials;
pub use tidal_auth::{TidalAuth, TidalTokens, TokenStore};
