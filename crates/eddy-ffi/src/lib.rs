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
        Arc::new(Self {
            inner: core::Resolver::new(config(
                cache_path,
                tidal_client_id,
                tidal_client_secret,
                tidal_user_store,
            )),
        })
    }

    /// Blocking network calls: call from a background thread.
    pub fn resolve(
        &self,
        url: String,
        service: Service,
        country: String,
    ) -> Result<Conversion, ResolveError> {
        Ok(self.inner.resolve(&url, service.into(), &country)?.into())
    }

    /// A link on `to` for a song playing elsewhere: exact when found, else a search
    /// link. `country` picks the Apple Music / TIDAL catalog. Never fails. Blocking
    /// network calls: call from a background thread.
    pub fn share_link(
        &self,
        now_playing: NowPlaying,
        to: ShareTarget,
        country: String,
    ) -> SongShare {
        self.inner
            .share_link(&now_playing.into(), to.into(), &country)
            .into()
    }
}

/// Tidal access, in order of preference: the user's login, developer credentials,
/// none. Empty strings count as missing (the app passes empty settings fields).
fn config(
    cache_path: String,
    tidal_client_id: Option<String>,
    tidal_client_secret: Option<String>,
    tidal_user_store: Option<Arc<dyn TidalTokenStore>>,
) -> core::ResolverConfig {
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
    core::ResolverConfig {
        cache_path: (!cache_path.is_empty()).then(|| PathBuf::from(cache_path)),
        tidal,
        explain: false,
    }
}

impl From<core::Conversion> for Conversion {
    fn from(c: core::Conversion) -> Self {
        Self {
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
        }
    }
}

// MARK: - Copy Link for Current Song

/// Where a copied link points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ShareTarget {
    Spotify,
    AppleMusic,
    Tidal,
}

impl From<ShareTarget> for core::ShareTarget {
    fn from(target: ShareTarget) -> Self {
        match target {
            ShareTarget::Spotify => Self::Spotify,
            ShareTarget::AppleMusic => Self::AppleMusic,
            ShareTarget::Tidal => Self::Tidal,
        }
    }
}

impl From<core::ShareTarget> for ShareTarget {
    fn from(target: core::ShareTarget) -> Self {
        match target {
            core::ShareTarget::Spotify => Self::Spotify,
            core::ShareTarget::AppleMusic => Self::AppleMusic,
            core::ShareTarget::Tidal => Self::Tidal,
        }
    }
}

/// What Tidal or Music is playing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct NowPlaying {
    pub title: String,
    /// Primary artist first.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

impl From<NowPlaying> for core::NowPlaying {
    fn from(n: NowPlaying) -> Self {
        Self {
            title: n.title,
            artists: n.artists,
            album: n.album,
            duration_ms: n.duration_ms,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SongShare {
    pub target: ShareTarget,
    /// A web link to the track, or a search link when not `exact`.
    pub url: String,
    pub exact: bool,
    pub title: String,
    pub artist: String,
    pub from_cache: bool,
}

impl From<core::SongShare> for SongShare {
    fn from(s: core::SongShare) -> Self {
        Self {
            target: s.target.into(),
            url: s.url,
            exact: s.exact,
            title: s.title,
            artist: s.artist,
            from_cache: s.from_cache,
        }
    }
}

// MARK: - Connect Tidal

/// The user's Tidal login. Stored by the app in the Keychain.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TidalTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    pub country: Option<String>,
}

impl From<core::TidalTokens> for TidalTokens {
    fn from(t: core::TidalTokens) -> Self {
        Self {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_at,
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

impl From<TidalLoginRequest> for core::tidal_auth::LoginRequest {
    fn from(r: TidalLoginRequest) -> Self {
        Self {
            authorize_url: r.authorize_url,
            client_id: r.client_id,
            redirect_uri: r.redirect_uri,
            state: r.state,
            code_verifier: r.code_verifier,
        }
    }
}

/// Whether a redirect belongs to this login. Any web page can open the redirect URI,
/// so others are ignored. No network.
#[uniffi::export]
pub fn tidal_callback_matches(request: TidalLoginRequest, callback_url: String) -> bool {
    core::tidal_auth::callback_matches(&request.into(), &callback_url)
}

/// Completes a login with the URL Tidal redirected to. Blocking network call.
#[uniffi::export]
pub fn tidal_finish_login(
    request: TidalLoginRequest,
    callback_url: String,
) -> Result<TidalTokens, TidalLoginError> {
    Ok(core::tidal_auth::finish_login(&request.into(), &callback_url)?.into())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn parses_links_for_swift() {
        assert_eq!(
            parse_link("https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=x".into()),
            ParsedLink::Spotify {
                kind: SpotifyKind::Track,
                id: "4uLU6hMCjMI75M1A2tKUQC".into(),
                canonical_url: "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC".into(),
                convertible: true,
            }
        );
        assert!(matches!(
            parse_link("spotify:playlist:37i9dQZF1DXcBWIGoYBM5M".into()),
            ParsedLink::Spotify {
                kind: SpotifyKind::Playlist,
                convertible: false,
                ..
            }
        ));
        assert_eq!(
            parse_link("https://spotify.link/abc".into()),
            ParsedLink::SpotifyShortLink {
                url: "https://spotify.link/abc".into()
            }
        );
        assert_eq!(
            parse_link("https://example.com".into()),
            ParsedLink::NotMusic
        );
        assert!(is_music_link("https://spotify.link/abc".into()));
        assert!(!is_music_link("https://example.com".into()));
    }

    #[test]
    fn converts_every_error() {
        let search = core::TargetUrls {
            native_url: "tidal://search?q=x".into(),
            web_url: "https://listen.tidal.com/search?q=x".into(),
        };
        let reasons = [
            (core::NotFoundReason::NoMatch, NotFoundReason::NoMatch),
            (
                core::NotFoundReason::NoCredentials,
                NotFoundReason::NoCredentials,
            ),
            (core::NotFoundReason::NoMetadata, NotFoundReason::NoMetadata),
            (
                core::NotFoundReason::AccountExpired,
                NotFoundReason::AccountExpired,
            ),
        ];
        for (core_reason, expected) in reasons {
            let error = ResolveError::from(core::ResolveError::NotFound {
                reason: core_reason,
                title: Some("Yeah!".into()),
                artist: Some("USHER".into()),
                search: Some(search.clone()),
            });
            let ResolveError::NotFound {
                reason,
                title,
                artist,
                search_native_url,
                search_web_url,
            } = error
            else {
                panic!("{error:?}")
            };
            assert_eq!(reason, expected);
            assert_eq!(title.as_deref(), Some("Yeah!"));
            assert_eq!(artist.as_deref(), Some("USHER"));
            assert_eq!(search_native_url.as_deref(), Some("tidal://search?q=x"));
            assert_eq!(
                search_web_url.as_deref(),
                Some("https://listen.tidal.com/search?q=x")
            );
        }
        let no_search = ResolveError::from(core::ResolveError::NotFound {
            reason: core::NotFoundReason::NoMetadata,
            title: None,
            artist: None,
            search: None,
        });
        assert!(matches!(
            no_search,
            ResolveError::NotFound {
                search_native_url: None,
                search_web_url: None,
                ..
            }
        ));
        assert!(matches!(
            core::ResolveError::NotSpotify.into(),
            ResolveError::NotSpotify
        ));
        assert!(matches!(
            core::ResolveError::Unsupported(core::SpotifyKind::Episode).into(),
            ResolveError::Unsupported {
                kind: SpotifyKind::Episode
            }
        ));
        assert!(matches!(
            core::ResolveError::AccountExpired.into(),
            ResolveError::AccountExpired
        ));
        let ResolveError::RateLimited { service } = core::ResolveError::RateLimited("TIDAL").into()
        else {
            panic!()
        };
        assert_eq!(service, "TIDAL");
        let ResolveError::Network { message } =
            core::ResolveError::Network("TIDAL returned HTTP 500".into()).into()
        else {
            panic!()
        };
        assert_eq!(message, "TIDAL returned HTTP 500");
    }

    #[test]
    fn converts_results() {
        let c = Conversion::from(core::Conversion {
            service: core::Service::Tidal,
            kind: core::SpotifyKind::Album,
            spotify_url: "https://open.spotify.com/album/x".into(),
            title: "Confessions".into(),
            artist: "USHER".into(),
            album: Some("Confessions (Expanded Edition)".into()),
            album_artist: Some("Usher".into()),
            native_url: "tidal://album/1".into(),
            web_url: "https://tidal.com/browse/album/1".into(),
            artist_native_url: Some("tidal://artist/2".into()),
            artist_web_url: Some("https://tidal.com/browse/artist/2".into()),
            from_cache: true,
        });
        assert_eq!(c.kind, SpotifyKind::Album);
        assert_eq!(c.spotify_url, "https://open.spotify.com/album/x");
        assert_eq!(c.title, "Confessions");
        assert_eq!(c.artist, "USHER");
        assert_eq!(c.album.as_deref(), Some("Confessions (Expanded Edition)"));
        assert_eq!(c.album_artist.as_deref(), Some("Usher"));
        assert_eq!(c.native_url, "tidal://album/1");
        assert_eq!(c.web_url, "https://tidal.com/browse/album/1");
        assert_eq!(c.artist_native_url.as_deref(), Some("tidal://artist/2"));
        assert_eq!(
            c.artist_web_url.as_deref(),
            Some("https://tidal.com/browse/artist/2")
        );
        assert!(c.from_cache);

        let s = SongShare::from(core::SongShare {
            target: core::ShareTarget::AppleMusic,
            url: "https://music.apple.com/de/album/x/1?i=2".into(),
            exact: true,
            title: "Yeah!".into(),
            artist: "USHER".into(),
            from_cache: false,
        });
        assert_eq!(s.target, ShareTarget::AppleMusic);
        assert_eq!(s.url, "https://music.apple.com/de/album/x/1?i=2");
        assert!(s.exact && !s.from_cache);
        assert_eq!((s.title.as_str(), s.artist.as_str()), ("Yeah!", "USHER"));

        let now = core::NowPlaying::from(NowPlaying {
            title: "Yeah!".into(),
            artists: vec!["USHER".into(), "Lil Jon".into()],
            album: Some("Confessions".into()),
            duration_ms: Some(250_000),
        });
        assert_eq!(now.artists, ["USHER", "Lil Jon"]);
        assert_eq!(now.album.as_deref(), Some("Confessions"));
        assert_eq!(now.duration_ms, Some(250_000));

        for target in [
            ShareTarget::Spotify,
            ShareTarget::AppleMusic,
            ShareTarget::Tidal,
        ] {
            assert_eq!(ShareTarget::from(core::ShareTarget::from(target)), target);
        }
        assert_eq!(core::Service::from(Service::Tidal), core::Service::Tidal);
        assert_eq!(
            core::Service::from(Service::AppleMusic),
            core::Service::AppleMusic
        );
    }

    #[derive(Default)]
    struct Keychain(Mutex<Option<TidalTokens>>);

    impl TidalTokenStore for Keychain {
        fn load(&self) -> Option<TidalTokens> {
            self.0.lock().unwrap().clone()
        }
        fn save(&self, tokens: TidalTokens) {
            *self.0.lock().unwrap() = Some(tokens);
        }
        fn clear(&self) {
            *self.0.lock().unwrap() = None;
        }
    }

    fn auth(config: core::ResolverConfig) -> String {
        format!("{:?}", config.tidal)
    }

    #[test]
    fn picks_tidal_access() {
        let store = || Some(Arc::new(Keychain::default()) as Arc<dyn TidalTokenStore>);
        let id = || Some("client".to_string());
        let secret = || Some("secret".to_string());
        // The user's login wins over developer credentials.
        assert_eq!(
            auth(config(String::new(), id(), secret(), store())),
            "Some(TidalAuth::User(..))"
        );
        assert_eq!(
            auth(config(String::new(), id(), secret(), None)),
            "Some(TidalAuth::ClientCredentials(..))"
        );
        // Empty settings fields count as missing.
        assert_eq!(
            auth(config(String::new(), id(), Some(String::new()), None)),
            "None"
        );
        assert_eq!(
            auth(config(
                String::new(),
                Some(String::new()),
                secret(),
                store()
            )),
            "None"
        );
        assert_eq!(auth(config(String::new(), None, None, store())), "None");

        assert_eq!(config(String::new(), None, None, None).cache_path, None);
        assert_eq!(
            config("/tmp/eddy.sqlite".into(), None, None, None).cache_path,
            Some(PathBuf::from("/tmp/eddy.sqlite"))
        );
    }

    #[test]
    fn token_store_round_trip() {
        let keychain = Arc::new(Keychain::default());
        let adapter = StoreAdapter(keychain.clone());
        let tokens = core::TidalTokens {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: 42,
            country: Some("SE".into()),
        };
        core::TokenStore::save(&adapter, tokens.clone());
        assert_eq!(core::TokenStore::load(&adapter), Some(tokens));
        core::TokenStore::clear(&adapter);
        assert_eq!(core::TokenStore::load(&adapter), None);
        assert!(keychain.load().is_none());
    }

    #[test]
    fn login_requests_survive_the_round_trip() {
        let request = tidal_login_request("client".into(), "eddy://auth/tidal".into());
        assert!(request
            .authorize_url
            .starts_with("https://login.tidal.com/authorize?"));
        let callback = format!("eddy://auth/tidal?code=c&state={}", request.state);
        assert!(tidal_callback_matches(request.clone(), callback));
        assert!(!tidal_callback_matches(
            request.clone(),
            "eddy://auth/tidal?code=c&state=forged".into()
        ));
        let core_request = core::tidal_auth::LoginRequest::from(request.clone());
        assert_eq!(core_request.code_verifier, request.code_verifier);
        assert!(matches!(
            tidal_finish_login(request, "eddy://auth/tidal?error=access_denied".into()),
            Err(TidalLoginError::StateMismatch)
        ));
        let errors = [
            (
                core::tidal_auth::AuthError::MissingCode,
                "the login response had no code",
            ),
            (
                core::tidal_auth::AuthError::Denied("the login was cancelled".into()),
                "the login was cancelled",
            ),
            (
                core::tidal_auth::AuthError::Network("offline".into()),
                "offline",
            ),
        ];
        for (error, message) in errors {
            assert_eq!(TidalLoginError::from(error).to_string(), message);
        }
    }
}
