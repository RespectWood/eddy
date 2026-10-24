//! The reverse direction: a Spotify link for the song playing in Tidal or Music, to
//! share with friends who use Spotify. No Spotify API (it needs Spotify Premium):
//!
//! 1. Deezer → the recording's ISRCs (title, artists, duration).
//! 2. MusicBrainz → Spotify track IDs linked to those ISRCs.
//! 3. Spotify's embed page → each candidate's metadata; the best match wins.
//! 4. Nothing found → a Spotify search link, which always works.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::explain::printable;
use crate::link::{SpotifyKind, SpotifyLink};
use crate::matching::{normalize, score, Candidate, THRESHOLD};
use crate::resolve::Resolver;
use crate::spotify::SpotifyMeta;
use crate::{deezer, musicbrainz, spotify};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotifyShare {
    /// `https://open.spotify.com/track/<id>`, or a search link when not `exact`.
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
    /// Never fails: lookup errors fall back to the search link. Blocking network calls.
    pub fn spotify_link(&self, now: &NowPlaying) -> SpotifyShare {
        let meta = now.meta();
        let key = cache_key(&meta);
        if let Some(mut hit) = self
            .with_cache(|c| c.get_share(&key))
            .flatten()
            .filter(|_| !self.explain)
        {
            hit.from_cache = true;
            return hit;
        }
        let exact = match self.find_on_spotify(&meta) {
            Ok(found) => found,
            Err(e) => {
                if self.explain {
                    eprintln!("exact lookup failed: {}", printable(&e.to_string()));
                }
                None
            }
        };
        let share = SpotifyShare {
            exact: exact.is_some(),
            url: exact.unwrap_or_else(|| search_url(&meta)),
            title: meta.title.clone(),
            artist: meta.artist_line(),
            from_cache: false,
        };
        // Only exact links are cached: MusicBrainz may learn the song later.
        if share.exact {
            self.with_cache(|c| c.put_share(&key, &share));
        }
        share
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

/// `https://open.spotify.com/search/<primary artist> <title>`
fn search_url(meta: &SpotifyMeta) -> String {
    let mut url = Url::parse("https://open.spotify.com/search/").expect("valid URL");
    let query = format!("{} {}", meta.primary_artist(), normalize(&meta.title));
    url.path_segments_mut()
        .expect("base URL")
        .pop_if_empty()
        .push(query.trim());
    url.to_string()
}

fn cache_key(meta: &SpotifyMeta) -> String {
    format!(
        "{}\u{1f}{}",
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
        assert_eq!(
            search_url(&usher().meta()),
            "https://open.spotify.com/search/USHER%20yeah"
        );
        let meta = NowPlaying {
            title: "Hyperballad".into(),
            artists: vec!["Björk".into()],
            ..Default::default()
        }
        .meta();
        assert_eq!(
            search_url(&meta),
            "https://open.spotify.com/search/Bj%C3%B6rk%20hyperballad"
        );
    }

    #[test]
    fn empty_player_falls_back_without_network() {
        let resolver = Resolver::new(ResolverConfig::default());
        let share = resolver.spotify_link(&NowPlaying::default());
        assert!(!share.exact);
    }

    /// Hits Deezer, MusicBrainz and Spotify: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_usher_gets_an_exact_link() {
        let resolver = Resolver::new(ResolverConfig::default());
        let share = resolver.spotify_link(&usher());
        assert!(share.exact, "{share:?}");
        assert!(
            share.url.starts_with("https://open.spotify.com/track/"),
            "{}",
            share.url
        );
    }
}
