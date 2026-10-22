use serde::Serialize;

use crate::link::SpotifyKind;
use crate::target::TargetUrls;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotFoundReason {
    /// The item doesn't exist on the target service (or we couldn't match it).
    NoMatch,
    /// Exact matching needs API credentials that aren't configured (Tidal).
    NoCredentials,
    /// Spotify's metadata for the item couldn't be read, so there is nothing to search for.
    NoMetadata,
    /// The user's Tidal login expired or was revoked; they need to reconnect.
    AccountExpired,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("not a Spotify link")]
    NotSpotify,

    #[error("Spotify {} links aren't converted", .0.as_str())]
    Unsupported(SpotifyKind),

    #[error("no match on the target service ({reason:?})")]
    NotFound {
        reason: NotFoundReason,
        title: Option<String>,
        artist: Option<String>,
        /// Search in the target service, when there's something to search for.
        search: Option<TargetUrls>,
    },

    /// The connected account's login is no longer valid. Resolving turns this into a
    /// `NotFound { reason: AccountExpired }` with a search fallback.
    #[error("the Tidal login expired; reconnect Tidal")]
    AccountExpired,

    #[error("rate limited by {0}")]
    RateLimited(&'static str),

    #[error("{0}")]
    Network(String),
}
