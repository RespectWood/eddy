//! UniFFI bindings exposing `openstream-core` to the Swift app.
//!
//! Kept deliberately thin: type conversions only, no logic.

use std::path::PathBuf;
use std::sync::Arc;

use openstream_core as core;

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SpotifyKind {
    Track,
    Album,
    Artist,
    Playlist,
    Episode,
    Show,
}

impl From<core::SpotifyKind> for SpotifyKind {
    fn from(kind: core::SpotifyKind) -> Self {
        match kind {
            core::SpotifyKind::Track => Self::Track,
            core::SpotifyKind::Album => Self::Album,
            core::SpotifyKind::Artist => Self::Artist,
            core::SpotifyKind::Playlist => Self::Playlist,
            core::SpotifyKind::Episode => Self::Episode,
            core::SpotifyKind::Show => Self::Show,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ParsedLink {
    Spotify {
        kind: SpotifyKind,
        id: String,
        canonical_url: String,
        convertible: bool,
    },
    SpotifyShortLink {
        url: String,
    },
    NotMusic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Service {
    AppleMusic,
    Tidal,
}

impl From<Service> for core::Service {
    fn from(service: Service) -> Self {
        match service {
            Service::AppleMusic => Self::AppleMusic,
            Service::Tidal => Self::Tidal,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Conversion {
    pub spotify_url: String,
    pub title: String,
    pub artist: String,
    pub native_url: String,
    pub web_url: String,
    pub from_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NotFoundReason {
    NoMatch,
    NoCredentials,
    NoMetadata,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ResolveError {
    #[error("not a Spotify link")]
    NotSpotify,
    #[error("this kind of Spotify link isn't converted")]
    Unsupported { kind: SpotifyKind },
    #[error("no match")]
    NotFound {
        reason: NotFoundReason,
        title: Option<String>,
        artist: Option<String>,
        search_native_url: Option<String>,
        search_web_url: Option<String>,
    },
    #[error("rate limited by {service}")]
    RateLimited { service: String },
    #[error("{message}")]
    Network { message: String },
}

impl From<core::ResolveError> for ResolveError {
    fn from(error: core::ResolveError) -> Self {
        match error {
            core::ResolveError::NotSpotify => Self::NotSpotify,
            core::ResolveError::Unsupported(kind) => Self::Unsupported { kind: kind.into() },
            core::ResolveError::NotFound {
                reason,
                title,
                artist,
                search,
            } => Self::NotFound {
                reason: match reason {
                    core::NotFoundReason::NoMatch => NotFoundReason::NoMatch,
                    core::NotFoundReason::NoCredentials => NotFoundReason::NoCredentials,
                    core::NotFoundReason::NoMetadata => NotFoundReason::NoMetadata,
                },
                title,
                artist,
                search_native_url: search.as_ref().map(|s| s.native_url.clone()),
                search_web_url: search.map(|s| s.web_url),
            },
            core::ResolveError::RateLimited(service) => Self::RateLimited {
                service: service.to_string(),
            },
            core::ResolveError::Network(message) => Self::Network { message },
        }
    }
}

/// Fast path, called for every clicked URL. Sync, no I/O.
#[uniffi::export]
pub fn is_music_link(url: String) -> bool {
    core::is_music_link(&url)
}

/// Sync, no I/O.
#[uniffi::export]
pub fn parse_link(url: String) -> ParsedLink {
    match core::parse(&url) {
        core::ParsedLink::Spotify(link) => ParsedLink::Spotify {
            kind: link.kind.into(),
            canonical_url: link.canonical_url(),
            convertible: link.kind.is_convertible(),
            id: link.id,
        },
        core::ParsedLink::SpotifyShortLink { url } => ParsedLink::SpotifyShortLink { url },
        core::ParsedLink::NotMusic => ParsedLink::NotMusic,
    }
}

#[derive(uniffi::Object)]
pub struct Resolver {
    inner: core::Resolver,
}

#[uniffi::export]
impl Resolver {
    /// `cache_path` empty disables the cache. Tidal credentials are optional.
    #[uniffi::constructor]
    pub fn new(
        cache_path: String,
        tidal_client_id: Option<String>,
        tidal_client_secret: Option<String>,
    ) -> Arc<Self> {
        let tidal = match (tidal_client_id, tidal_client_secret) {
            (Some(client_id), Some(client_secret))
                if !client_id.is_empty() && !client_secret.is_empty() =>
            {
                Some(core::TidalCredentials {
                    client_id,
                    client_secret,
                })
            }
            _ => None,
        };
        let cache_path = (!cache_path.is_empty()).then(|| PathBuf::from(cache_path));
        Arc::new(Self {
            inner: core::Resolver::new(core::ResolverConfig { cache_path, tidal }),
        })
    }

    /// Blocking network calls: call from a background thread.
    pub fn resolve(
        &self,
        url: String,
        service: Service,
        country: String,
    ) -> Result<Conversion, ResolveError> {
        let c = self.inner.resolve(&url, service.into(), &country)?;
        Ok(Conversion {
            spotify_url: c.spotify_url,
            title: c.title,
            artist: c.artist,
            native_url: c.native_url,
            web_url: c.web_url,
            from_cache: c.from_cache,
        })
    }
}
