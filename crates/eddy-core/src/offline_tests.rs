//! Whole lookups, from a clicked link to the result, against recorded responses
//! (`tests/fixtures/cassettes`, see `replay.rs`). No network: an unrecorded request
//! fails the test.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::http::Http;
use crate::replay::{offline, replaying, Exchange, Replay};
use crate::tidal_auth::{self, AuthError, DEFAULT_SCOPES};
use crate::{
    Conversion, NotFoundReason, NowPlaying, ResolveError, Resolver, ResolverConfig, Service,
    ShareTarget, SpotifyKind, TidalAuth, TidalCredentials, TidalTokens, TokenStore,
};

const RICK: &str = "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC";
const USHER: &str = "https://open.spotify.com/track/5rb9QrpfcKFHM1EUbSIurX";
const ITUNES: &str = "GET https://itunes.apple.com/search";
const DEEZER_USHER: &str = "GET https://api.deezer.com/search/track?q=USHER+yeah";
const MUSICBRAINZ_USHER: &str =
    "GET https://musicbrainz.org/ws/2/isrc/USAR10301423?fmt=json&inc=url-rels";
const TIDAL_TOKEN: &str = "POST https://auth.tidal.com/v1/oauth2/token";
const TIDAL_TRACKS: &str = "GET https://openapi.tidal.com/v2/tracks";
/// What "Copy Link for Current Song → Spotify" finds for USHER – Yeah!.
const USHER_ON_SPOTIFY: &str = "https://open.spotify.com/track/1M18Qdzl9xFVRmUm66zn34";
/// The original album's track (*Confessions (Expanded Edition)*), not a compilation.
const USHER_ON_TIDAL: &str = "tidal://track/11337915";

fn resolver(http: Http) -> Resolver {
    Resolver::with_http(ResolverConfig::default(), http)
}

fn tidal_resolver(http: Http) -> Resolver {
    with_tidal(
        http,
        TidalAuth::ClientCredentials(TidalCredentials {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
        }),
    )
}

fn with_tidal(http: Http, auth: TidalAuth) -> Resolver {
    Resolver::with_http(
        ResolverConfig {
            tidal: Some(auth),
            ..Default::default()
        },
        http,
    )
}

fn usher_playing() -> NowPlaying {
    NowPlaying {
        title: "Yeah! (feat. Lil Jon & Ludacris)".into(),
        artists: vec!["USHER".into(), "Lil Jon".into(), "Ludacris".into()],
        album: Some("Confessions (Expanded Edition)".into()),
        duration_ms: Some(250_000),
    }
}

fn hosts(replay: &Replay) -> Vec<String> {
    replay
        .requests()
        .iter()
        .map(|r| {
            let url = r.split_once(' ').unwrap().1;
            url::Url::parse(url)
                .unwrap()
                .host_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

/// A cache file of its own, deleted when dropped.
struct TempCache(std::path::PathBuf);

impl TempCache {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("eddy-test-{}-{name}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempCache {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Stands in for the app's Keychain.
#[derive(Default)]
struct MemoryStore {
    tokens: Mutex<Option<TidalTokens>>,
    cleared: Mutex<bool>,
}

impl MemoryStore {
    fn with(tokens: TidalTokens) -> Arc<Self> {
        Arc::new(Self {
            tokens: Mutex::new(Some(tokens)),
            ..Default::default()
        })
    }

    fn tokens(&self) -> Option<TidalTokens> {
        self.tokens.lock().unwrap().clone()
    }
}

impl TokenStore for MemoryStore {
    fn load(&self) -> Option<TidalTokens> {
        self.tokens()
    }
    fn save(&self, tokens: TidalTokens) {
        *self.tokens.lock().unwrap() = Some(tokens);
    }
    fn clear(&self) {
        *self.tokens.lock().unwrap() = None;
        *self.cleared.lock().unwrap() = true;
    }
}

fn user_tokens(expires_in: i64) -> TidalTokens {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    TidalTokens {
        access_token: "user-access".into(),
        refresh_token: Some("user-refresh".into()),
        expires_at: (now + expires_in).max(0) as u64,
        country: Some("SE".into()),
    }
}

fn user_auth(store: Arc<MemoryStore>) -> TidalAuth {
    TidalAuth::User {
        client_id: "test-client".into(),
        store,
    }
}

fn not_found(err: ResolveError) -> (NotFoundReason, Option<crate::TargetUrls>) {
    match err {
        ResolveError::NotFound { reason, search, .. } => (reason, search),
        other => panic!("expected NotFound, got {other:?}"),
    }
}

// MARK: - Spotify → Apple Music

#[test]
fn apple_music_track() {
    let (http, replay) = offline("apple_music_track_rick_astley");
    let c = resolver(http)
        .resolve(&format!("{RICK}?si=abc"), Service::AppleMusic, "de")
        .unwrap();
    assert_eq!(c.title, "Never Gonna Give You Up");
    assert_eq!(c.artist, "Rick Astley");
    assert_eq!(c.kind, SpotifyKind::Track);
    assert_eq!(c.spotify_url, RICK);
    assert!(
        c.native_url
            .starts_with("music://music.apple.com/de/album/")
            && c.native_url.contains("?i="),
        "{}",
        c.native_url
    );
    assert_eq!(c.native_url.replace("music://", "https://"), c.web_url);
    assert!(!c.from_cache);
    assert_eq!(replay.requests().len(), 2, "{:?}", replay.requests());
}

#[test]
fn apple_music_track_from_a_localized_link() {
    let (http, _) = offline("apple_music_track_rick_astley");
    let plain = resolver(http.clone())
        .resolve(RICK, Service::AppleMusic, "DE")
        .unwrap();
    let localized = resolver(http)
        .resolve(
            "https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC?si=x",
            Service::AppleMusic,
            "DE",
        )
        .unwrap();
    assert_eq!(plain.native_url, localized.native_url);
}

#[test]
fn apple_music_album_falls_back_to_song_results() {
    let (http, replay) = offline("apple_music_album_dark_side");
    let c = resolver(http)
        .resolve(
            "https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv",
            Service::AppleMusic,
            "US",
        )
        .unwrap();
    assert_eq!(c.kind, SpotifyKind::Album);
    assert!(c.native_url.contains("/album/"), "{}", c.native_url);
    assert!(!c.native_url.contains("?i="), "an album, not a track");
    assert!(
        c.album
            .as_deref()
            .unwrap()
            .contains("Dark Side of the Moon"),
        "{c:?}"
    );
    // The album search alone finds nothing that matches; song results carry the album.
    let requests = replay.requests();
    assert!(requests[1].contains("entity=album"), "{requests:?}");
    assert!(requests[2].contains("entity=song"), "{requests:?}");
}

#[test]
fn apple_music_remastered_title() {
    let (http, _) = offline("apple_music_track_remastered");
    let c = resolver(http)
        .resolve(
            "https://open.spotify.com/track/7tFiyTwD0nx5a1eklYtX2J",
            Service::AppleMusic,
            "US",
        )
        .unwrap();
    assert_eq!(c.title, "Bohemian Rhapsody - Remastered 2011");
    assert!(c.native_url.contains("?i="), "{}", c.native_url);
    assert!(
        !c.album_artist
            .as_deref()
            .is_some_and(crate::matching::is_various_artists),
        "{c:?}"
    );
}

/// Kendrick Lamar – HUMBLE. is explicit on Spotify, but the iTunes Search API only
/// returns the clean versions (recorded 2026-10-25). The album's clean version wins
/// over covers and lullaby versions.
#[test]
fn apple_music_explicit_track_without_an_explicit_result() {
    let (http, _) = offline("apple_music_track_explicit");
    let c = resolver(http)
        .resolve(
            "https://open.spotify.com/track/7KXjTSCq5nL1LoYtL7XAwS",
            Service::AppleMusic,
            "US",
        )
        .unwrap();
    assert!(c.native_url.ends_with("?i=1440882165"), "{}", c.native_url);
    assert_eq!(c.album.as_deref(), Some("DAMN."));
}

#[test]
fn apple_music_prefers_the_original_album_over_compilations() {
    let (http, _) = offline("apple_music_track_usher");
    let c = resolver(http)
        .resolve(USHER, Service::AppleMusic, "SE")
        .unwrap();
    assert!(c.native_url.ends_with("?i=386153478"), "{}", c.native_url);
    assert_eq!(c.album.as_deref(), Some("Confessions (Expanded Edition)"));
}

/// The ten songs in `original_releases.txt` are all on "Various Artists" compilations
/// too; each must open on the artist's own release.
#[test]
fn original_releases_not_compilations() {
    let (http, _) = offline("original_releases");
    let resolver = resolver(http);
    let lines = include_str!("../tests/fixtures/original_releases.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    let mut count = 0;
    for line in lines {
        let url = line.split('\t').next().unwrap();
        let c = resolver.resolve(url, Service::AppleMusic, "SE").unwrap();
        assert!(
            !c.album_artist
                .as_deref()
                .is_some_and(crate::matching::is_various_artists),
            "{line} → {:?}",
            c.album
        );
        count += 1;
    }
    assert_eq!(count, 10);
}

// MARK: - Spotify → TIDAL

#[test]
fn tidal_without_credentials_searches_without_asking_tidal() {
    let (http, replay) = offline("tidal_without_credentials");
    let err = resolver(http)
        .resolve(RICK, Service::Tidal, "DE")
        .unwrap_err();
    let (reason, search) = not_found(err);
    assert_eq!(reason, NotFoundReason::NoCredentials);
    let search = search.unwrap();
    assert!(
        search
            .native_url
            .starts_with("tidal://search?q=Rick+Astley"),
        "{}",
        search.native_url
    );
    assert_eq!(hosts(&replay), ["open.spotify.com"]);
}

#[test]
fn tidal_with_credentials_opens_the_original_release() {
    let (http, replay) = offline("tidal_usher_resolve");
    let c = tidal_resolver(http)
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap();
    assert_eq!(c.native_url, USHER_ON_TIDAL);
    assert_eq!(c.web_url, "https://tidal.com/browse/track/11337915");
    assert_eq!(c.album.as_deref(), Some("Confessions (Expanded Edition)"));
    assert_eq!(c.album_artist.as_deref(), Some("Usher"));
    let artist = c.artist_native_url.unwrap();
    assert!(artist.starts_with("tidal://artist/"), "{artist}");
    let requests = replay.requests();
    assert!(
        requests.iter().any(|r| r.starts_with(TIDAL_TOKEN)),
        "{requests:?}"
    );
    assert!(
        requests
            .iter()
            .any(|r| r.contains("filter%5Bisrc%5D=USAR10301423")),
        "the Deezer ISRC is used: {requests:?}"
    );
}

#[test]
fn tidal_with_a_user_login() {
    let (http, replay) = offline("tidal_usher_resolve");
    let store = MemoryStore::with(user_tokens(3600));
    let c = with_tidal(http, user_auth(store.clone()))
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap();
    assert_eq!(c.native_url, USHER_ON_TIDAL);
    // A fresh login needs no token request.
    assert!(!replay.requests().iter().any(|r| r.starts_with("POST")));
    assert_eq!(store.tokens().unwrap().access_token, "user-access");
}

#[test]
fn tidal_refreshes_an_expired_login() {
    let (http, replay) = offline("tidal_usher_resolve");
    let store = MemoryStore::with(user_tokens(-10));
    let c = with_tidal(http, user_auth(store.clone()))
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap();
    assert_eq!(c.native_url, USHER_ON_TIDAL);
    let tokens = store.tokens().unwrap();
    assert_eq!(
        tokens.access_token, "REDACTED",
        "the refreshed token is saved"
    );
    assert!(tokens.is_fresh());
    // The response had no refresh token, so the old one stays.
    assert_eq!(tokens.refresh_token.as_deref(), Some("user-refresh"));
    assert_eq!(tokens.country.as_deref(), Some("SE"));
    assert_eq!(
        replay
            .requests()
            .iter()
            .filter(|r| r.starts_with(TIDAL_TOKEN))
            .count(),
        1
    );
}

#[test]
fn tidal_revoked_login_searches_and_asks_to_reconnect() {
    let replay = Replay::cassette("tidal_usher_resolve")
        .first(Exchange::json(TIDAL_TOKEN, json!({"error": "invalid_grant"})).with_status(400));
    let (http, replay) = replaying(replay);
    let store = MemoryStore::with(user_tokens(-10));
    let err = with_tidal(http, user_auth(store.clone()))
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap_err();
    let (reason, search) = not_found(err);
    assert_eq!(reason, NotFoundReason::AccountExpired);
    assert!(search.unwrap().native_url.starts_with("tidal://search?q="));
    assert!(*store.cleared.lock().unwrap(), "the dead login is removed");
    assert!(!hosts(&replay).contains(&"openapi.tidal.com".to_string()));
}

#[test]
fn tidal_without_a_stored_login_asks_to_reconnect() {
    let (http, replay) = offline("tidal_usher_resolve");
    let err = with_tidal(http, user_auth(Arc::default()))
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap_err();
    assert_eq!(not_found(err).0, NotFoundReason::AccountExpired);
    assert!(!hosts(&replay).iter().any(|h| h.contains("tidal")));
}

#[test]
fn tidal_rate_limit() {
    let replay = Replay::cassette("tidal_usher_resolve").first(Exchange::status(TIDAL_TRACKS, 429));
    let (http, _) = replaying(replay);
    let err = tidal_resolver(http)
        .resolve(USHER, Service::Tidal, "SE")
        .unwrap_err();
    assert!(matches!(err, ResolveError::RateLimited("TIDAL")), "{err:?}");
}

/// A rejected token is dropped, so the next lookup gets a new one.
#[test]
fn tidal_rejected_token_is_renewed() {
    let replay = Replay::cassette("tidal_usher_resolve").first(Exchange::status(TIDAL_TRACKS, 401));
    let (http, replay) = replaying(replay);
    let resolver = tidal_resolver(http);
    let err = resolver.resolve(USHER, Service::Tidal, "SE").unwrap_err();
    assert_eq!(err.to_string(), "TIDAL returned HTTP 401");
    let c = resolver.resolve(USHER, Service::Tidal, "SE").unwrap();
    assert_eq!(c.native_url, USHER_ON_TIDAL);
    let tokens = replay
        .requests()
        .iter()
        .filter(|r| r.starts_with(TIDAL_TOKEN))
        .count();
    assert_eq!(tokens, 2);
}

// MARK: - Not found and errors

#[test]
fn no_match_opens_a_search() {
    let replay = Replay::cassette("apple_music_track_rick_astley").first(Exchange::json(
        ITUNES,
        json!({"resultCount": 0, "results": []}),
    ));
    let (http, _) = replaying(replay);
    let err = resolver(http)
        .resolve(RICK, Service::AppleMusic, "DE")
        .unwrap_err();
    let ResolveError::NotFound {
        reason,
        title,
        artist,
        search,
    } = err
    else {
        panic!("{err:?}")
    };
    assert_eq!(reason, NotFoundReason::NoMatch);
    assert_eq!(title.as_deref(), Some("Never Gonna Give You Up"));
    assert_eq!(artist.as_deref(), Some("Rick Astley"));
    assert_eq!(
        search.unwrap().native_url,
        "music://music.apple.com/de/search?term=Rick%20Astley%20never%20gonna%20give%20you%20up"
    );
}

#[test]
fn unreadable_spotify_page_has_nothing_to_search_for() {
    let replay = Replay::new().with(Exchange::text(
        "GET https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC",
        "<html>no data</html>",
    ));
    let (http, replay) = replaying(replay);
    let err = resolver(http)
        .resolve(RICK, Service::AppleMusic, "DE")
        .unwrap_err();
    let (reason, search) = not_found(err);
    assert_eq!(reason, NotFoundReason::NoMetadata);
    assert!(search.is_none());
    assert_eq!(replay.requests().len(), 1);
}

/// Each failure the app reacts to (it opens the original link and says why).
#[test]
fn service_errors() {
    let cases: Vec<(Exchange, &str)> = vec![
        (Exchange::status(ITUNES, 429), "rate limited by Apple Music"),
        (
            Exchange::status(ITUNES, 500),
            "Apple Music returned HTTP 500",
        ),
        (
            Exchange::text(ITUNES, "<html>maintenance</html>"),
            "Apple Music: invalid response: expected value at line 1 column 1",
        ),
        (
            Exchange::text(ITUNES, ""),
            "Apple Music: invalid response: EOF while parsing a value at line 1 column 0",
        ),
        (
            Exchange::transport_error(ITUNES, "timed out reading response"),
            "Apple Music: timed out reading response",
        ),
    ];
    for (exchange, expected) in cases {
        let replay = Replay::cassette("apple_music_track_rick_astley").first(exchange);
        let (http, _) = replaying(replay);
        let err = resolver(http)
            .resolve(RICK, Service::AppleMusic, "DE")
            .unwrap_err();
        assert_eq!(err.to_string(), expected);
    }
}

#[test]
fn spotify_errors() {
    let embed = "GET https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC";
    let (http, _) = replaying(Replay::new().with(Exchange::status(embed, 404)));
    let err = resolver(http)
        .resolve(RICK, Service::AppleMusic, "DE")
        .unwrap_err();
    assert_eq!(err.to_string(), "Spotify returned HTTP 404");
    let (http, _) = replaying(Replay::new().with(Exchange::status(embed, 429)));
    let err = resolver(http)
        .resolve(RICK, Service::AppleMusic, "DE")
        .unwrap_err();
    assert!(
        matches!(err, ResolveError::RateLimited("Spotify")),
        "{err:?}"
    );
}

// MARK: - Short links

fn rick_with(exchanges: Vec<Exchange>) -> (Http, Arc<Replay>) {
    replaying(exchanges.into_iter().fold(
        Replay::cassette("apple_music_track_rick_astley"),
        Replay::with,
    ))
}

#[test]
fn short_link_redirect() {
    let (http, replay) = rick_with(vec![Exchange::redirect(
        "GET https://spotify.link/abc",
        &format!("{RICK}?si=x&utm_source=copy-link"),
    )]);
    let c = resolver(http)
        .resolve("https://spotify.link/abc", Service::AppleMusic, "DE")
        .unwrap();
    assert_eq!(c.title, "Never Gonna Give You Up");
    let requests = replay.requests();
    assert_eq!(requests[0], "GET https://spotify.link/abc");
    // The content page itself isn't fetched, only its embed page.
    assert!(!requests
        .iter()
        .any(|r| r.contains("open.spotify.com/track")));
}

#[test]
fn short_links_are_fetched_over_https() {
    let (http, replay) = rick_with(vec![Exchange::redirect(
        "GET https://spotify.link/abc",
        RICK,
    )]);
    resolver(http)
        .resolve("http://spotify.link/abc", Service::AppleMusic, "DE")
        .unwrap();
    assert_eq!(replay.requests()[0], "GET https://spotify.link/abc");
}

/// `spoti.fi` → relative redirect → a branch.io page with the link in its HTML.
#[test]
fn short_link_chain_ending_in_a_page() {
    let (http, replay) = rick_with(vec![
        Exchange::redirect("GET https://spoti.fi/xyz", "/app/xyz"),
        Exchange::text(
            "GET https://spoti.fi/app/xyz",
            r#"<html><script>{"$canonical_url":"https:\/\/open.spotify.com\/track\/4uLU6hMCjMI75M1A2tKUQC?si=1"}</script></html>"#,
        ),
    ]);
    let c = resolver(http)
        .resolve("https://spoti.fi/xyz", Service::AppleMusic, "DE")
        .unwrap();
    assert_eq!(c.title, "Never Gonna Give You Up");
    assert_eq!(replay.requests()[1], "GET https://spoti.fi/app/xyz");
}

#[test]
fn short_link_to_somewhere_else_is_not_converted() {
    let (http, _) = replaying(
        Replay::new()
            .with(Exchange::redirect(
                "GET https://spotify.link/abc",
                "https://example.com/promo",
            ))
            .with(Exchange::text(
                "GET https://example.com/promo",
                "<html>hi</html>",
            )),
    );
    let err = resolver(http)
        .resolve("https://spotify.link/abc", Service::AppleMusic, "DE")
        .unwrap_err();
    assert!(matches!(err, ResolveError::NotSpotify), "{err:?}");
}

#[test]
fn short_link_failures() {
    let cases = [
        (
            Exchange::redirect("GET https://spotify.link/abc", "ftp://example.com/x"),
            "Spotify short link: redirect to a non-web link",
        ),
        (
            Exchange::redirect("GET https://spotify.link/abc", "https://spotify.link/abc"),
            "Spotify short link: too many redirects",
        ),
        (
            Exchange::redirect("GET https://spotify.link/abc", "").with_status(302),
            "Spotify short link: too many redirects",
        ),
        (
            Exchange::status("GET https://spotify.link/abc", 404),
            "Spotify short link returned HTTP 404",
        ),
    ];
    for (exchange, expected) in cases {
        let (http, _) = replaying(Replay::new().with(exchange));
        let err = resolver(http)
            .resolve("https://spotify.link/abc", Service::AppleMusic, "DE")
            .unwrap_err();
        assert_eq!(err.to_string(), expected);
    }
    let mut no_target = Exchange::redirect("GET https://spotify.link/abc", "x");
    no_target.location = None;
    let (http, _) = replaying(Replay::new().with(no_target));
    let err = resolver(http)
        .resolve("https://spotify.link/abc", Service::AppleMusic, "DE")
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Spotify short link: redirect without a target"
    );
}

// MARK: - Cache

#[test]
fn cached_conversions_need_no_requests() {
    let cache = TempCache::new("conversions");
    let (http, replay) = offline("apple_music_track_rick_astley");
    let resolver = Resolver::with_http(
        ResolverConfig {
            cache_path: Some(cache.0.clone()),
            ..Default::default()
        },
        http,
    );
    let first = resolver.resolve(RICK, Service::AppleMusic, "DE").unwrap();
    let requests = replay.requests().len();
    let second = resolver
        .resolve(&format!("{RICK}?si=other"), Service::AppleMusic, "DE")
        .unwrap();
    assert!(second.from_cache);
    assert_eq!(
        Conversion {
            from_cache: false,
            ..second
        },
        first
    );
    assert_eq!(replay.requests().len(), requests, "no new request");
}

#[test]
fn exact_share_links_are_cached() {
    let cache = TempCache::new("shares");
    let (http, replay) = offline("share_usher_apple_music");
    let resolver = Resolver::with_http(
        ResolverConfig {
            cache_path: Some(cache.0.clone()),
            ..Default::default()
        },
        http,
    );
    let first = resolver.share_link(&usher_playing(), ShareTarget::AppleMusic, "DE");
    assert!(first.exact && !first.from_cache);
    let second = resolver.share_link(&usher_playing(), ShareTarget::AppleMusic, "DE");
    assert!(second.from_cache);
    assert_eq!(second.url, first.url);
    assert_eq!(replay.requests().len(), 1);
}

#[test]
fn share_misses_are_remembered_for_ten_minutes() {
    let replay = Replay::new().with(Exchange::json(DEEZER_USHER, json!({"data": []})));
    let (http, replay) = replaying(replay);
    let resolver = resolver(http);
    let share = resolver.share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert!(!share.exact);
    assert_eq!(share.url, "https://open.spotify.com/search/USHER%20yeah");
    assert_eq!(replay.requests().len(), 1);

    resolver.share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert_eq!(replay.requests().len(), 1, "the miss is remembered");

    for at in resolver.share_misses.lock().unwrap().values_mut() {
        *at = Instant::now() - Duration::from_secs(10 * 60 + 1);
    }
    resolver.share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert_eq!(
        replay.requests().len(),
        2,
        "looked up again after 10 minutes"
    );
}

// MARK: - Share links ("Copy Link for Current Song")

#[test]
fn share_to_spotify() {
    let (http, replay) = offline("share_usher_spotify");
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert!(share.exact, "{share:?}");
    // MusicBrainz links the recording to two Spotify tracks (same audio, two
    // releases); the better match by title and duration wins.
    assert_eq!(share.url, USHER_ON_SPOTIFY);
    assert_eq!(share.title, "Yeah! (feat. Lil Jon & Ludacris)");
    assert_eq!(share.artist, "USHER, Lil Jon, Ludacris");
    // Deezer → MusicBrainz → both Spotify candidates checked on their embed pages.
    assert_eq!(
        hosts(&replay),
        [
            "api.deezer.com",
            "musicbrainz.org",
            "open.spotify.com",
            "open.spotify.com"
        ]
    );
}

#[test]
fn share_to_apple_music() {
    let (http, _) = offline("share_usher_apple_music");
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::AppleMusic, "DE");
    assert!(share.exact, "{share:?}");
    assert!(
        share.url.starts_with("https://music.apple.com/de/album/") && share.url.contains("?i="),
        "{}",
        share.url
    );
}

#[test]
fn share_to_tidal() {
    let (http, _) = offline("tidal_usher_resolve");
    let share = tidal_resolver(http).share_link(&usher_playing(), ShareTarget::Tidal, "SE");
    assert!(share.exact, "{share:?}");
    assert_eq!(share.url, "https://tidal.com/browse/track/11337915");
}

#[test]
fn share_to_tidal_without_credentials_is_a_search() {
    let (http, replay) = replaying(Replay::new());
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::Tidal, "SE");
    assert!(!share.exact);
    assert_eq!(share.url, "https://listen.tidal.com/search?q=USHER%20yeah");
    assert!(replay.requests().is_empty());
}

/// MusicBrainz is often busy at peak times; one retry.
#[test]
fn share_retries_a_busy_musicbrainz_once() {
    let replay =
        Replay::cassette("share_usher_spotify").first(Exchange::status(MUSICBRAINZ_USHER, 503));
    let (http, replay) = replaying(replay);
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert!(share.exact, "{share:?}");
    let musicbrainz = hosts(&replay)
        .iter()
        .filter(|h| *h == "musicbrainz.org")
        .count();
    assert_eq!(musicbrainz, 2);
}

#[test]
fn share_falls_back_to_search_when_musicbrainz_stays_busy() {
    let replay = Replay::cassette("share_usher_spotify")
        .first(Exchange::status(MUSICBRAINZ_USHER, 503))
        .first(Exchange::status(MUSICBRAINZ_USHER, 503));
    let (http, _) = replaying(replay);
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert!(!share.exact);
    assert_eq!(share.url, "https://open.spotify.com/search/USHER%20yeah");
}

/// A candidate whose embed page fails is skipped, not fatal: the other one wins.
#[test]
fn share_skips_a_removed_spotify_track() {
    let replay = Replay::cassette("share_usher_spotify").first(Exchange::status(
        "GET https://open.spotify.com/embed/track/1M18Qdzl9xFVRmUm66zn34",
        404,
    ));
    let (http, _) = replaying(replay);
    let share = resolver(http).share_link(&usher_playing(), ShareTarget::Spotify, "DE");
    assert!(share.exact);
    assert_eq!(share.url, USHER);
}

// MARK: - Connect TIDAL

fn login() -> (crate::tidal_auth::LoginRequest, String) {
    let request = tidal_auth::login_request("test-client", "eddy://auth/tidal", DEFAULT_SCOPES);
    let callback = format!("eddy://auth/tidal?code=the-code&state={}", request.state);
    (request, callback)
}

const PROFILE: &str = "GET https://openapi.tidal.com/v2/users/me";

#[test]
fn login_trades_the_code_for_tokens() {
    let (request, callback) = login();
    let (http, replay) = replaying(
        Replay::new()
            .with(Exchange::json(
                TIDAL_TOKEN,
                json!({"access_token": "REDACTED", "refresh_token": "REDACTED", "expires_in": 3600}),
            ))
            .with(Exchange::json(
                PROFILE,
                json!({"data": {"id": "1", "attributes": {"country": "NO"}}}),
            )),
    );
    let tokens = tidal_auth::finish_login_with(&http, &request, &callback).unwrap();
    assert!(tokens.is_fresh());
    assert!(tokens.refresh_token.is_some());
    assert_eq!(tokens.country.as_deref(), Some("NO"));
    assert_eq!(replay.requests(), [TIDAL_TOKEN, PROFILE]);
}

/// The profile needs a scope new apps don't have; the login works without it.
#[test]
fn login_without_profile_access() {
    let (request, callback) = login();
    let (http, _) = replaying(
        Replay::new()
            .with(Exchange::json(
                TIDAL_TOKEN,
                json!({"access_token": "REDACTED", "expires_in": 3600}),
            ))
            .with(Exchange::status(PROFILE, 403)),
    );
    let tokens = tidal_auth::finish_login_with(&http, &request, &callback).unwrap();
    assert_eq!(tokens.country, None);
    assert_eq!(tokens.refresh_token, None);
}

#[test]
fn login_failures() {
    let (request, callback) = login();
    let (http, _) = replaying(
        Replay::new()
            .with(Exchange::json(TIDAL_TOKEN, json!({"error": "invalid_grant"})).with_status(400)),
    );
    let err = tidal_auth::finish_login_with(&http, &request, &callback).unwrap_err();
    assert_eq!(
        err,
        AuthError::Denied(r#"HTTP 400: {"error":"invalid_grant"}"#.into())
    );

    let (http, _) = replaying(Replay::new().with(Exchange::status(TIDAL_TOKEN, 502)));
    let err = tidal_auth::finish_login_with(&http, &request, &callback).unwrap_err();
    assert_eq!(
        err,
        AuthError::Network("TIDAL login returned HTTP 502".into())
    );

    // A forged redirect never reaches the network.
    let (http, replay) = replaying(Replay::new());
    let forged = "eddy://auth/tidal?code=evil&state=wrong";
    let err = tidal_auth::finish_login_with(&http, &request, forged).unwrap_err();
    assert_eq!(err, AuthError::StateMismatch);
    assert!(replay.requests().is_empty());
}

// MARK: - Every error variant

/// `AccountExpired` comes out of TIDAL lookups and is turned into `NotFound` by the
/// resolver (see `tidal_without_a_stored_login_asks_to_reconnect`).
#[test]
fn tidal_lookups_report_an_expired_account() {
    let (http, _) = replaying(Replay::new());
    let tidal = crate::tidal::Tidal::new(user_auth(Arc::default()));
    let meta = crate::spotify::SpotifyMeta {
        kind: SpotifyKind::Track,
        title: "Yeah!".into(),
        artists: vec!["USHER".into()],
        duration_ms: None,
        explicit: None,
        track_count: None,
    };
    let err = tidal
        .find_track(&http, &meta, Some("USAR10301423"), "", "SE", false)
        .unwrap_err();
    assert!(matches!(err, ResolveError::AccountExpired), "{err:?}");
}
