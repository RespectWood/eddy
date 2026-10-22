//! Resolves a Spotify link to the same item on another service.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::error::{NotFoundReason, ResolveError};
use crate::http::Http;
use crate::link::{parse, ParsedLink, SpotifyKind, SpotifyLink};
use crate::spotify::SpotifyMeta;
use crate::target::{search_urls, tidal_urls, ItemKind, Service, TargetUrls};
use crate::tidal::{Tidal, TidalCredentials};
use crate::{deezer, expand, itunes, spotify};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversion {
    pub service: Service,
    pub spotify_url: String,
    pub title: String,
    pub artist: String,
    pub native_url: String,
    pub web_url: String,
    #[serde(default)]
    pub from_cache: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ResolverConfig {
    /// SQLite cache location. `None` disables caching.
    pub cache_path: Option<PathBuf>,
    /// Enables exact Tidal matching. Without it, Tidal lookups fall back to search.
    pub tidal: Option<TidalCredentials>,
}

pub struct Resolver {
    http: Http,
    cache: Option<Mutex<Cache>>,
    tidal: Option<Tidal>,
}

impl Resolver {
    pub fn new(config: ResolverConfig) -> Self {
        // A broken cache shouldn't stop conversions, so open errors disable it.
        let cache = config
            .cache_path
            .and_then(|p| Cache::open(&p).ok())
            .map(Mutex::new);
        Self {
            http: Http::new(),
            cache,
            tidal: config.tidal.map(Tidal::new),
        }
    }

    /// Resolves any clicked URL. Blocking: call it off the main thread.
    pub fn resolve(
        &self,
        url: &str,
        service: Service,
        country: &str,
    ) -> Result<Conversion, ResolveError> {
        let link = match parse(url) {
            ParsedLink::Spotify(link) => link,
            ParsedLink::SpotifyShortLink { url } => {
                expand::expand(&self.http, &url)?.ok_or(ResolveError::NotSpotify)?
            }
            ParsedLink::NotMusic => return Err(ResolveError::NotSpotify),
        };
        self.resolve_link(&link, service, country)
    }

    pub fn resolve_link(
        &self,
        link: &SpotifyLink,
        service: Service,
        country: &str,
    ) -> Result<Conversion, ResolveError> {
        if !link.kind.is_convertible() {
            return Err(ResolveError::Unsupported(link.kind));
        }
        let country = normalize_country(country);
        if let Some(mut hit) = self
            .with_cache(|c| c.get(link, service, &country))
            .flatten()
        {
            hit.from_cache = true;
            return Ok(hit);
        }
        let meta = spotify::fetch(&self.http, link)?.ok_or(ResolveError::NotFound {
            reason: NotFoundReason::NoMetadata,
            title: None,
            artist: None,
            search: None,
        })?;
        let found = match service {
            Service::AppleMusic => itunes::find(&self.http, &meta, &country)?,
            Service::Tidal => self.find_on_tidal(&meta, &country)?,
        };
        let Some(urls) = found else {
            let reason = if service == Service::Tidal && self.tidal.is_none() {
                NotFoundReason::NoCredentials
            } else {
                NotFoundReason::NoMatch
            };
            return Err(not_found(reason, &meta, service, &country));
        };
        let conversion = Conversion {
            service,
            spotify_url: link.canonical_url(),
            title: meta.title.clone(),
            artist: meta.artist_line(),
            native_url: urls.native_url,
            web_url: urls.web_url,
            from_cache: false,
        };
        self.with_cache(|c| c.put(link, service, &country, &conversion));
        Ok(conversion)
    }

    fn find_on_tidal(
        &self,
        meta: &SpotifyMeta,
        country: &str,
    ) -> Result<Option<TargetUrls>, ResolveError> {
        let Some(tidal) = &self.tidal else {
            return Ok(None);
        };
        let Some(code) = deezer::find_code(&self.http, meta)? else {
            return Ok(None);
        };
        let kind = match meta.kind {
            SpotifyKind::Album => ItemKind::Album,
            _ => ItemKind::Track,
        };
        Ok(tidal
            .find(&self.http, kind, &code, country)?
            .map(|id| tidal_urls(kind, &id)))
    }

    fn with_cache<T>(&self, f: impl FnOnce(&Cache) -> T) -> Option<T> {
        let cache = self.cache.as_ref()?;
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        Some(f(&guard))
    }
}

fn not_found(
    reason: NotFoundReason,
    meta: &SpotifyMeta,
    service: Service,
    country: &str,
) -> ResolveError {
    let query = format!(
        "{} {}",
        meta.primary_artist(),
        crate::matching::normalize(&meta.title)
    );
    ResolveError::NotFound {
        reason,
        title: Some(meta.title.clone()),
        artist: Some(meta.artist_line()),
        search: Some(search_urls(service, &query, country)),
    }
}

/// Two-letter uppercase country code; falls back to US.
fn normalize_country(country: &str) -> String {
    let c = country.trim();
    if c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic()) {
        c.to_ascii_uppercase()
    } else {
        "US".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_country() {
        assert_eq!(normalize_country("de"), "DE");
        assert_eq!(normalize_country(""), "US");
        assert_eq!(normalize_country("DEU"), "US");
    }

    #[test]
    fn rejects_unsupported_links_without_network() {
        let resolver = Resolver::new(ResolverConfig::default());
        let err = resolver
            .resolve(
                "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
                Service::Tidal,
                "DE",
            )
            .unwrap_err();
        assert!(matches!(
            err,
            ResolveError::Unsupported(SpotifyKind::Playlist)
        ));
        assert!(matches!(
            resolver
                .resolve("https://example.com", Service::Tidal, "DE")
                .unwrap_err(),
            ResolveError::NotSpotify
        ));
    }

    /// Hits the real Spotify and iTunes APIs: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_apple_music_track() {
        let resolver = Resolver::new(ResolverConfig::default());
        let c = resolver
            .resolve(
                "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=x",
                Service::AppleMusic,
                "DE",
            )
            .unwrap();
        assert_eq!(c.title, "Never Gonna Give You Up");
        assert!(
            c.native_url.starts_with("music://music.apple.com/de/"),
            "{}",
            c.native_url
        );
    }

    #[test]
    #[ignore]
    fn live_apple_music_album() {
        let resolver = Resolver::new(ResolverConfig::default());
        let c = resolver
            .resolve(
                "https://open.spotify.com/album/4LH4d3cOWNNsVw41Gqt2kv",
                Service::AppleMusic,
                "US",
            )
            .unwrap();
        assert!(c.native_url.contains("/album/"), "{}", c.native_url);
    }

    #[test]
    #[ignore]
    fn live_tidal_without_credentials_falls_back_to_search() {
        let resolver = Resolver::new(ResolverConfig::default());
        let err = resolver
            .resolve(
                "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
                Service::Tidal,
                "DE",
            )
            .unwrap_err();
        let ResolveError::NotFound { reason, search, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!(reason, NotFoundReason::NoCredentials);
        assert!(search
            .unwrap()
            .native_url
            .starts_with("tidal://search?q=Rick+Astley"));
    }
}
