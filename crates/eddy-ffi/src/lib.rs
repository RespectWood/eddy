//! UniFFI bindings exposing `eddy-core` to the Swift app.
//!
//! Kept deliberately thin: type conversions only, no logic.

use std::path::PathBuf;
use std::sync::Arc;

use eddy_core as core;

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
    pub kind: SpotifyKind,
    pub spotify_url: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub native_url: String,
    pub web_url: String,
    /// The primary artist's page (Tidal only).
    pub artist_native_url: Option<String>,
    pub artist_web_url: Option<String>,
    pub from_cache: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NotFoundReason {
    NoMatch,
    NoCredentials,
    NoMetadata,
    AccountExpired,
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
    #[error("the Tidal login expired")]
    AccountExpired,
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
                    core::NotFoundReason::AccountExpired => NotFoundReason::AccountExpired,
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
            core::ResolveError::AccountExpired => Self::AccountExpired,
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
    /// `cache_path` empty disables the cache. Tidal access, in order of preference:
    /// the user's login (`tidal_user_store` + `tidal_client_id`), developer credentials
    /// (`tidal_client_id` + `tidal_client_secret`), or none (Tidal falls back to search).
    #[uniffi::constructor]
    pub fn new(
        cache_path: String,
        tidal_client_id: Option<String>,
        tidal_client_secret: Option<String>,
        tidal_user_store: Option<Arc<dyn TidalTokenStore>>,
    ) -> Arc<Self> {
        let client_id = tidal_client_id.filter(|id| !id.is_empty());
        let secret = tidal_client_secret.filter(|s| !s.is_empty());
        let tidal = match (client_id, secret, tidal_user_store) {
            (Some(client_id), _, Some(store)) => Some(core::TidalAuth::User {
                client_id,
                store: Arc::new(StoreAdapter(store)),
            }),
            (Some(client_id), Some(client_secret), None) => {
                Some(core::TidalAuth::ClientCredentials(core::TidalCredentials {
                    client_id,
                    client_secret,
                }))
            }
            _ => None,
        };
        let cache_path = (!cache_path.is_empty()).then(|| PathBuf::from(cache_path));
        Arc::new(Self {
            inner: core::Resolver::new(core::ResolverConfig {
                cache_path,
                tidal,
                explain: false,
            }),
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
            kind: c.kind.into(),
            spotify_url: c.spotify_url,
            title: c.title,
            artist: c.artist,
            album: c.album,
            album_artist: c.album_artist,
            native_url: c.native_url,
            web_url: c.web_url,
            artist_native_url: c.artist_native_url,
            artist_web_url: c.artist_web_url,
            from_cache: c.from_cache,
        })
    }

    /// A Spotify link for a song playing elsewhere: exact when found, else a Spotify
    /// search link. Never fails. Blocking network calls: call from a background thread.
    pub fn spotify_link(&self, now_playing: NowPlaying) -> SpotifyShare {
        let s = self.inner.spotify_link(&core::NowPlaying {
            title: now_playing.title,
            artists: now_playing.artists,
            album: now_playing.album,
            duration_ms: now_playing.duration_ms,
        });
        SpotifyShare {
            url: s.url,
            exact: s.exact,
            title: s.title,
            artist: s.artist,
            from_cache: s.from_cache,
        }
    }
}

// MARK: - Copy Spotify Link

/// What Tidal or Music is playing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NowPlaying {
    pub title: String,
    /// Primary artist first.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SpotifyShare {
    /// A track link, or a Spotify search link when not `exact`.
    pub url: String,
    pub exact: bool,
    pub title: String,
    pub artist: String,
    pub from_cache: bool,
}

// MARK: - Connect Tidal

/// The user's Tidal login. Stored by the app in the Keychain.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TidalTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    pub user_name: Option<String>,
    pub country: Option<String>,
}

impl From<core::TidalTokens> for TidalTokens {
    fn from(t: core::TidalTokens) -> Self {
        Self {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_at,
            user_name: t.user_name,
            country: t.country,
        }
    }
}

impl From<TidalTokens> for core::TidalTokens {
    fn from(t: TidalTokens) -> Self {
        Self {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_at,
            user_name: t.user_name,
            country: t.country,
        }
    }
}

/// A login in progress; kept by the app until Tidal redirects back.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TidalLoginRequest {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    pub code_verifier: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TidalLoginError {
    #[error("{message}")]
    Denied { message: String },
    #[error("the login response didn't match this login")]
    StateMismatch,
    #[error("the login response had no code")]
    MissingCode,
    #[error("{message}")]
    Network { message: String },
}

impl From<core::tidal_auth::AuthError> for TidalLoginError {
    fn from(e: core::tidal_auth::AuthError) -> Self {
        use core::tidal_auth::AuthError;
        match e {
            AuthError::Denied(message) => Self::Denied { message },
            AuthError::StateMismatch => Self::StateMismatch,
            AuthError::MissingCode => Self::MissingCode,
            AuthError::Network(message) => Self::Network { message },
        }
    }
}

/// Implemented by the app (Keychain). The core calls `save` after refreshing tokens
/// and `clear` when the login is no longer valid.
#[uniffi::export(with_foreign)]
pub trait TidalTokenStore: Send + Sync {
    fn load(&self) -> Option<TidalTokens>;
    fn save(&self, tokens: TidalTokens);
    fn clear(&self);
}

struct StoreAdapter(Arc<dyn TidalTokenStore>);

impl core::TokenStore for StoreAdapter {
    fn load(&self) -> Option<core::TidalTokens> {
        self.0.load().map(Into::into)
    }
    fn save(&self, tokens: core::TidalTokens) {
        self.0.save(tokens.into());
    }
    fn clear(&self) {
        self.0.clear();
    }
}

/// Starts a login. Open `authorize_url` in a browser sheet.
#[uniffi::export]
pub fn tidal_login_request(client_id: String, redirect_uri: String) -> TidalLoginRequest {
    let r = core::tidal_auth::login_request(
        &client_id,
        &redirect_uri,
        core::tidal_auth::DEFAULT_SCOPES,
    );
    TidalLoginRequest {
        authorize_url: r.authorize_url,
        client_id: r.client_id,
        redirect_uri: r.redirect_uri,
        state: r.state,
        code_verifier: r.code_verifier,
    }
}

/// Completes a login with the URL Tidal redirected to. Blocking network call.
#[uniffi::export]
pub fn tidal_finish_login(
    request: TidalLoginRequest,
    callback_url: String,
) -> Result<TidalTokens, TidalLoginError> {
    let request = core::tidal_auth::LoginRequest {
        authorize_url: request.authorize_url,
        client_id: request.client_id,
        redirect_uri: request.redirect_uri,
        state: request.state,
        code_verifier: request.code_verifier,
    };
    Ok(core::tidal_auth::finish_login(&request, &callback_url)?.into())
}
