//! Resolves a Spotify link to the same item on another service.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cache::Cache;
use crate::error::{NotFoundReason, ResolveError};
use crate::http::Http;
use crate::link::{parse, ParsedLink, SpotifyKind, SpotifyLink};
use crate::spotify::SpotifyMeta;
use crate::target::{search_urls, tidal_urls, ItemKind, Service, TargetUrls};
use crate::tidal::Tidal;
use crate::tidal_auth::TidalAuth;
use crate::{deezer, expand, itunes, spotify};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversion {
    pub service: Service,
    pub kind: SpotifyKind,
    pub spotify_url: String,
    pub title: String,
    pub artist: String,
    /// The release that opens, e.g. "Confessions (Expanded Edition)".
    pub album: Option<String>,
    pub album_artist: Option<String>,
    /// Opens the item: the track on its album page, or the album.
    pub native_url: String,
    pub web_url: String,
    /// The primary artist's page, when the service supports it (Tidal).
    pub artist_native_url: Option<String>,
    pub artist_web_url: Option<String>,
    #[serde(default)]
    pub from_cache: bool,
}

/// What a service lookup found.
pub(crate) struct Found {
    pub urls: TargetUrls,
    pub artist_urls: Option<TargetUrls>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolverConfig {
    /// SQLite cache location. `None` disables caching.
    pub cache_path: Option<PathBuf>,
    /// Enables exact Tidal matching (developer credentials or the user's login).
    /// Without it, Tidal lookups fall back to search.
    pub tidal: Option<TidalAuth>,
    /// Print every candidate and its score to stderr (CLI `--explain`). Skips the cache.
    pub explain: bool,
}

pub struct Resolver {
    pub(crate) http: Http,
    cache: Option<Mutex<Cache>>,
    tidal: Option<Tidal>,
    pub(crate) explain: bool,
    /// Share links found to have no exact match, and when (see `share_link`).
    pub(crate) share_misses: Mutex<HashMap<String, Instant>>,
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
            explain: config.explain,
            share_misses: Mutex::default(),
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
            .filter(|_| !self.explain)
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
            Service::AppleMusic => itunes::find(&self.http, &meta, &country, self.explain)?,
            Service::Tidal => match self.find_on_tidal(&meta, &country) {
                Err(ResolveError::AccountExpired) => {
                    return Err(not_found(
                        NotFoundReason::AccountExpired,
                        &meta,
                        service,
                        &country,
                    ))
                }
                other => other?,
            },
        };
        let Some(found) = found else {
            let reason = if service == Service::Tidal && self.tidal.is_none() {
                NotFoundReason::NoCredentials
            } else {
                NotFoundReason::NoMatch
            };
            return Err(not_found(reason, &meta, service, &country));
        };
        let conversion = Conversion {
            service,
            kind: link.kind,
            spotify_url: link.canonical_url(),
            title: meta.title.clone(),
            artist: meta.artist_line(),
            album: found.album,
            album_artist: found.album_artist,
            native_url: found.urls.native_url,
            web_url: found.urls.web_url,
            artist_native_url: found.artist_urls.as_ref().map(|u| u.native_url.clone()),
            artist_web_url: found.artist_urls.map(|u| u.web_url),
            from_cache: false,
        };
        self.with_cache(|c| c.put(link, service, &country, &conversion));
        Ok(conversion)
    }

    pub(crate) fn find_on_tidal(
        &self,
        meta: &SpotifyMeta,
        country: &str,
    ) -> Result<Option<Found>, ResolveError> {
        let Some(tidal) = &self.tidal else {
            return Ok(None);
        };
        // Deezer supplies the ISRC/UPC and the album name. Tracks can still be found
        // through Tidal search without it; albums need the UPC.
        let deezer = deezer::find_code(&self.http, meta)?;
        let album_hint = deezer
            .as_ref()
            .map(|d| d.album_title.as_str())
            .unwrap_or_default();
        let (kind, item) = match meta.kind {
            SpotifyKind::Album => {
                let Some(deezer) = &deezer else {
                    return Ok(None);
                };
                let album = tidal.find_album(
                    &self.http,
                    meta,
                    &deezer.code,
                    album_hint,
                    country,
                    self.explain,
                )?;
                (ItemKind::Album, album)
            }
            _ => {
                let isrc = deezer.as_ref().map(|d| d.code.as_str());
                let track =
                    tidal.find_track(&self.http, meta, isrc, album_hint, country, self.explain)?;
                (ItemKind::Track, track)
            }
        };
        Ok(item.and_then(|item| {
            Some(Found {
                urls: tidal_urls(kind, &item.id)?,
                artist_urls: item
                    .artist_id
                    .and_then(|id| tidal_urls(ItemKind::Artist, &id)),
                album: item.album_title,
                album_artist: item.album_artist,
            })
        }))
    }

    pub(crate) fn with_cache<T>(&self, f: impl FnOnce(&Cache) -> T) -> Option<T> {
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
pub(crate) fn normalize_country(country: &str) -> String {
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

    /// Popular tracks that also appear on many compilations. Each must open on the
    /// artist's own release. Needs `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET` for Tidal.
    #[test]
    #[ignore]
    fn live_original_releases_not_compilations() {
        let tidal = match (
            std::env::var("TIDAL_CLIENT_ID"),
            std::env::var("TIDAL_CLIENT_SECRET"),
        ) {
            (Ok(client_id), Ok(client_secret)) => {
                Some(TidalAuth::ClientCredentials(crate::TidalCredentials {
                    client_id,
                    client_secret,
                }))
            }
            _ => None,
        };
        let services: &[Service] = if tidal.is_some() {
            &[Service::AppleMusic, Service::Tidal]
        } else {
            &[Service::AppleMusic]
        };
        let resolver = Resolver::new(ResolverConfig {
            tidal,
            ..Default::default()
        });
        let tracks = include_str!("../tests/fixtures/original_releases.txt");
        let mut failures = Vec::new();
        for line in tracks
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        {
            let url = line.split('\t').next().unwrap();
            for &service in services {
                match resolver.resolve(url, service, "SE") {
                    Ok(c)
                        if c.album_artist
                            .as_deref()
                            .is_some_and(crate::matching::is_various_artists) =>
                    {
                        failures.push(format!("{line} → {service:?}: compilation {:?}", c.album))
                    }
                    Ok(_) => {}
                    Err(e) => failures.push(format!("{line} → {service:?}: {e}")),
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
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
