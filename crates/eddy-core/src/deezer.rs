//! ISRC / UPC lookup through the public Deezer API (no credentials needed).
//!
//! Spotify's embed page doesn't expose ISRCs, so Deezer is used to find the
//! recording's ISRC (tracks) or the release's UPC (albums), which Tidal can match exactly.

use serde::Deserialize;

use crate::error::ResolveError;
use crate::http::{json, Http};
use crate::link::SpotifyKind;
use crate::matching::{best, normalize, score, Candidate, THRESHOLD};
use crate::spotify::SpotifyMeta;

const API: &str = "https://api.deezer.com";

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    id: u64,
    title: String,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default)]
    nb_tracks: Option<u32>,
    #[serde(default)]
    explicit_lyrics: Option<bool>,
    /// Included in track search results.
    #[serde(default)]
    isrc: Option<String>,
    artist: Artist,
    /// Track results only.
    #[serde(default)]
    album: Option<AlbumRef>,
}

#[derive(Debug, Deserialize)]
struct Artist {
    name: String,
}

#[derive(Debug, Deserialize)]
struct AlbumRef {
    title: String,
}

/// The identifying code of the matched recording/release, plus the album it was
/// found on, which helps pick the original release among a code's many matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeezerMatch {
    /// ISRC for tracks, UPC for albums.
    pub code: String,
    pub album_title: String,
}

#[derive(Debug, Deserialize)]
struct Codes {
    isrc: Option<String>,
    upc: Option<String>,
}

/// Finds the ISRC (tracks) or UPC (albums) of the matching Deezer item.
pub(crate) fn find_code(
    http: &Http,
    meta: &SpotifyMeta,
) -> Result<Option<DeezerMatch>, ResolveError> {
    let path = match meta.kind {
        SpotifyKind::Album => "album",
        _ => "track",
    };
    // Plain text search: Deezer's advanced syntax (`artist:"…" track:"…"`) returns nothing.
    let query = format!("{} {}", meta.primary_artist(), normalize(&meta.title));
    let response: SearchResponse = json(
        http.get(&format!("{API}/search/{path}")).query("q", &query),
        "Deezer",
    )?;
    let Some(item) = best(meta, response.data, |i| Candidate {
        title: &i.title,
        artist: &i.artist.name,
        duration_ms: i.duration.map(|s| s * 1000),
        explicit: i.explicit_lyrics,
        track_count: i.nb_tracks,
        compilation: false,
    }) else {
        return Ok(None);
    };
    let album_title = match meta.kind {
        SpotifyKind::Album => item.title.clone(),
        _ => item
            .album
            .as_ref()
            .map(|a| a.title.clone())
            .unwrap_or_default(),
    };
    let code = match item.isrc {
        Some(isrc) => Some(isrc),
        None => {
            let codes: Codes = json(http.get(&format!("{API}/{path}/{}", item.id)), "Deezer")?;
            match meta.kind {
                SpotifyKind::Album => codes.upc,
                _ => codes.isrc,
            }
        }
    };
    Ok(code.map(|code| DeezerMatch { code, album_title }))
}

/// ISRCs of every matching track result, best match first. A song often has several
/// (original, remaster, single), and only some may be known elsewhere.
pub(crate) fn find_isrcs(
    http: &Http,
    meta: &SpotifyMeta,
    max: usize,
) -> Result<Vec<String>, ResolveError> {
    let query = format!("{} {}", meta.primary_artist(), normalize(&meta.title));
    let response: SearchResponse = json(
        http.get(&format!("{API}/search/track")).query("q", &query),
        "Deezer",
    )?;
    let mut scored: Vec<(i32, String)> = response
        .data
        .into_iter()
        .filter_map(|i| {
            let s = score(
                meta,
                &Candidate {
                    title: &i.title,
                    artist: &i.artist.name,
                    duration_ms: i.duration.map(|s| s * 1000),
                    explicit: i.explicit_lyrics,
                    ..Default::default()
                },
            )?;
            Some((s, i.isrc?))
        })
        .filter(|(s, _)| *s >= THRESHOLD)
        .collect();
    // Stable, so Deezer's order decides ties.
    scored.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    let mut isrcs: Vec<String> = Vec::new();
    for (_, isrc) in scored {
        if !isrcs.contains(&isrc) {
            isrcs.push(isrc);
        }
    }
    isrcs.truncate(max);
    Ok(isrcs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits the real Deezer API: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_finds_isrc() {
        let meta = SpotifyMeta {
            kind: SpotifyKind::Track,
            title: "Never Gonna Give You Up".into(),
            artists: vec!["Rick Astley".into()],
            duration_ms: Some(213_573),
            explicit: Some(false),
            track_count: None,
        };
        // Deezer's results don't always include the original release, so any Rick Astley
        // (label prefix GBARL) recording of the song counts.
        let found = find_code(&Http::new(), &meta).unwrap().unwrap();
        assert!(found.code.starts_with("GBARL"), "{}", found.code);
        assert!(!found.album_title.is_empty());
    }
}
