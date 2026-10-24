//! The reverse direction: a link for the song playing in Tidal or Music, to share with
//! friends ("Copy Link for Current Song"). Links point to Spotify, Apple Music or TIDAL.
//!
//! Spotify, without the Spotify API (it needs Spotify Premium):
//! 1. Deezer → the recording's ISRCs (title, artists, duration).
//! 2. MusicBrainz → Spotify track IDs linked to those ISRCs.
//! 3. Spotify's embed page → each candidate's metadata; the best match wins.
//!
//! Apple Music and TIDAL use the same lookups as clicked Spotify links (iTunes Search;
//! Deezer ISRC → TIDAL API). Nothing found → a search link, which always works.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::explain::printable;
use crate::link::{SpotifyKind, SpotifyLink};
use crate::matching::{normalize, score, Candidate, THRESHOLD};
use crate::resolve::{normalize_country, Resolver};
use crate::spotify::SpotifyMeta;
use crate::target::{search_urls, Service};
use crate::{deezer, itunes, musicbrainz, spotify};

/// Deezer ISRCs to try on MusicBrainz (one request per second each).
const MAX_ISRCS: usize = 3;
/// Spotify candidates to check on the embed page.
const MAX_CANDIDATES: usize = 4;

/// What the player shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NowPlaying {
    pub title: String,
    /// Primary artist first.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
}

/// Where a copied link points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShareTarget {
    Spotify,
    AppleMusic,
    Tidal,
}

impl ShareTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spotify => "spotify",
            Self::AppleMusic => "apple-music",
            Self::Tidal => "tidal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "spotify" => Some(Self::Spotify),
            other => Service::parse(other).map(Self::from),
        }
    }
}

impl From<Service> for ShareTarget {
    fn from(service: Service) -> Self {
        match service {
            Service::AppleMusic => Self::AppleMusic,
            Service::Tidal => Self::Tidal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongShare {
    pub target: ShareTarget,
    /// A web link to the track (`open.spotify.com`, `music.apple.com`, `tidal.com`),
    /// or a search link when not `exact`. Web links open for the recipient anywhere.
    pub url: String,
    pub exact: bool,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub from_cache: bool,
}

impl NowPlaying {
    fn meta(&self) -> SpotifyMeta {
        SpotifyMeta {
            kind: SpotifyKind::Track,
            title: self.title.trim().to_string(),
            artists: self
                .artists
                .iter()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            duration_ms: self.duration_ms,
            explicit: None,
            track_count: None,
        }
    }
}

impl Resolver {
    /// A link to the song on `to`. `country` picks the Apple Music / TIDAL catalog
    /// (Spotify links work everywhere). Never fails: lookup errors fall back to a search
    /// link. Blocking network calls.
    pub fn share_link(&self, now: &NowPlaying, to: ShareTarget, country: &str) -> SongShare {
        let meta = now.meta();
        let country = normalize_country(country);
        let key = cache_key(&meta, to, &country);
        if let Some(mut hit) = self
            .with_cache(|c| c.get_share(&key))
            .flatten()
            .filter(|_| !self.explain)
        {
            hit.from_cache = true;
            return hit;
        }
        let exact = match self.find_share(&meta, to, &country) {
            Ok(found) => found,
            Err(e) => {
                if self.explain {
                    eprintln!("exact lookup failed: {}", printable(&e.to_string()));
                }
                None
            }
        };
        let share = SongShare {
            target: to,
            exact: exact.is_some(),
            url: exact.unwrap_or_else(|| search_url(&meta, to, &country)),
            title: meta.title.clone(),
            artist: meta.artist_line(),
            from_cache: false,
        };
        // Only exact links are cached: the song may be found later (MusicBrainz learns
        // new links; a Tidal login may be connected).
        if share.exact {
            self.with_cache(|c| c.put_share(&key, &share));
        }
        share
    }

    /// The song's web link on `to`, if found exactly.
    fn find_share(
        &self,
        meta: &SpotifyMeta,
        to: ShareTarget,
        country: &str,
    ) -> Result<Option<String>, crate::ResolveError> {
        if meta.title.is_empty() || meta.artists.is_empty() {
            return Ok(None);
        }
        Ok(match to {
            ShareTarget::Spotify => self.find_on_spotify(meta)?,
            ShareTarget::AppleMusic => {
                itunes::find(&self.http, meta, country, self.explain)?.map(|f| f.urls.web_url)
            }
            ShareTarget::Tidal => self.find_on_tidal(meta, country)?.map(|f| f.urls.web_url),
        })
    }

    fn find_on_spotify(&self, meta: &SpotifyMeta) -> Result<Option<String>, crate::ResolveError> {
        if meta.title.is_empty() || meta.artists.is_empty() {
            return Ok(None);
        }
        let isrcs = deezer::find_isrcs(&self.http, meta, MAX_ISRCS)?;
        if self.explain {
            eprintln!("Deezer ISRCs: {isrcs:?}");
        }
        let mut ids: Vec<String> = Vec::new();
        let mut failure = None;
        for isrc in &isrcs {
            // A busy server for one ISRC shouldn't end the lookup; try the next one.
            match musicbrainz::spotify_track_ids(&self.http, isrc) {
                Ok(found) => {
                    for id in found {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
                Err(e) => failure = Some(e),
            }
            // The best-matching ISRC's links are enough; others only help when it has none.
            if !ids.is_empty() {
                break;
            }
        }
        if let (true, Some(e)) = (ids.is_empty(), failure) {
            return Err(e);
        }
        if self.explain {
            eprintln!("MusicBrainz Spotify tracks: {ids:?}");
        }
        let mut best: Option<(i32, String)> = None;
        for id in ids.into_iter().take(MAX_CANDIDATES) {
            let link = SpotifyLink {
                kind: SpotifyKind::Track,
                id,
            };
            // A removed track or a failed request only drops this candidate.
            let Ok(Some(found)) = spotify::fetch(&self.http, &link) else {
                if self.explain {
                    eprintln!("  {}: no metadata", link.id);
                }
                continue;
            };
            let artists = found.artist_line();
            let s = score(
                meta,
                &Candidate {
                    title: &found.title,
                    artist: &artists,
                    duration_ms: found.duration_ms,
                    ..Default::default()
                },
            );
            if self.explain {
                eprintln!(
                    "  {:?}  {}: {} — {}",
                    s,
                    link.id,
                    printable(&found.title),
                    printable(&artists)
                );
            }
            if let Some(s) = s.filter(|s| *s >= THRESHOLD) {
                if best.as_ref().is_none_or(|(b, _)| s > *b) {
                    best = Some((s, link.canonical_url()));
                }
            }
        }
        Ok(best.map(|(_, url)| url))
    }
}

/// A search for "<primary artist> <title>" on `to`, as a web link.
fn search_url(meta: &SpotifyMeta, to: ShareTarget, country: &str) -> String {
    let query = format!("{} {}", meta.primary_artist(), normalize(&meta.title));
    match to {
        // `https://open.spotify.com/search/<query>`
        ShareTarget::Spotify => {
            let mut url = Url::parse("https://open.spotify.com/search/").expect("valid URL");
            url.path_segments_mut()
                .expect("base URL")
                .pop_if_empty()
                .push(query.trim());
            url.to_string()
        }
        ShareTarget::AppleMusic => search_urls(Service::AppleMusic, &query, country).web_url,
        ShareTarget::Tidal => search_urls(Service::Tidal, &query, country).web_url,
    }
}

/// Target, catalog country (none for Spotify: its links work everywhere), artist, title.
fn cache_key(meta: &SpotifyMeta, to: ShareTarget, country: &str) -> String {
    let country = if to == ShareTarget::Spotify {
        ""
    } else {
        country
    };
    format!(
        "{}\u{1f}{country}\u{1f}{}\u{1f}{}",
        to.as_str(),
        normalize(meta.primary_artist()),
        normalize(&meta.title)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolverConfig;

    fn usher() -> NowPlaying {
        NowPlaying {
            title: "Yeah! (feat. Lil Jon & Ludacris)".into(),
            artists: vec!["USHER".into(), "Lil Jon".into(), "Ludacris".into()],
            album: Some("Confessions (Expanded Edition)".into()),
            duration_ms: Some(250_000),
        }
    }

    #[test]
    fn builds_search_urls() {
        let usher = usher().meta();
        assert_eq!(
            search_url(&usher, ShareTarget::Spotify, "DE"),
            "https://open.spotify.com/search/USHER%20yeah"
        );
        assert_eq!(
            search_url(&usher, ShareTarget::AppleMusic, "DE"),
            "https://music.apple.com/de/search?term=USHER%20yeah"
        );
        assert_eq!(
            search_url(&usher, ShareTarget::Tidal, "DE"),
            "https://listen.tidal.com/search?q=USHER%20yeah"
        );
        let meta = NowPlaying {
            title: "Hyperballad".into(),
            artists: vec!["Björk".into()],
            ..Default::default()
        }
        .meta();
        assert_eq!(
            search_url(&meta, ShareTarget::Spotify, "DE"),
            "https://open.spotify.com/search/Bj%C3%B6rk%20hyperballad"
        );
    }

    #[test]
    fn cache_keys_separate_targets_and_catalogs() {
        let meta = usher().meta();
        let spotify = cache_key(&meta, ShareTarget::Spotify, "DE");
        assert_eq!(spotify, cache_key(&meta, ShareTarget::Spotify, "US"));
        assert_ne!(
            cache_key(&meta, ShareTarget::Tidal, "DE"),
            cache_key(&meta, ShareTarget::Tidal, "US")
        );
        assert_ne!(spotify, cache_key(&meta, ShareTarget::AppleMusic, "DE"));
    }

    #[test]
    fn parses_targets() {
        assert_eq!(ShareTarget::parse("Spotify"), Some(ShareTarget::Spotify));
        assert_eq!(ShareTarget::parse("music"), Some(ShareTarget::AppleMusic));
        assert_eq!(ShareTarget::parse("tidal"), Some(ShareTarget::Tidal));
        assert_eq!(ShareTarget::parse("deezer"), None);
    }

    #[test]
    fn empty_player_falls_back_without_network() {
        let resolver = Resolver::new(ResolverConfig::default());
        for to in [
            ShareTarget::Spotify,
            ShareTarget::AppleMusic,
            ShareTarget::Tidal,
        ] {
            let share = resolver.share_link(&NowPlaying::default(), to, "DE");
            assert!(!share.exact);
            assert_eq!(share.target, to);
        }
    }

    /// Hits Deezer, MusicBrainz and Spotify: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_usher_gets_an_exact_link() {
        let resolver = Resolver::new(ResolverConfig::default());
        let share = resolver.share_link(&usher(), ShareTarget::Spotify, "DE");
        assert!(share.exact, "{share:?}");
        assert!(
            share.url.starts_with("https://open.spotify.com/track/"),
            "{}",
            share.url
        );
    }

    /// Hits the iTunes Search API: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_usher_gets_an_apple_music_link() {
        let resolver = Resolver::new(ResolverConfig::default());
        let share = resolver.share_link(&usher(), ShareTarget::AppleMusic, "DE");
        assert!(share.exact, "{share:?}");
        assert!(
            share.url.starts_with("https://music.apple.com/de/album/"),
            "{}",
            share.url
        );
    }
}
