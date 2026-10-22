//! Apple Music lookup through the iTunes Search API (no credentials needed).

use serde::Deserialize;

use crate::error::ResolveError;
use crate::http::{json, Http};
use crate::link::SpotifyKind;
use crate::matching::{best, search_query, Candidate};
use crate::spotify::SpotifyMeta;
use crate::target::{apple_music_urls, TargetUrls};

const SEARCH_URL: &str = "https://itunes.apple.com/search";

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Item>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    track_name: Option<String>,
    collection_name: Option<String>,
    #[serde(default)]
    artist_name: String,
    track_time_millis: Option<u64>,
    track_explicitness: Option<String>,
    collection_explicitness: Option<String>,
    track_view_url: Option<String>,
    collection_view_url: Option<String>,
    track_count: Option<u32>,
}

pub(crate) fn find(
    http: &Http,
    meta: &SpotifyMeta,
    country: &str,
) -> Result<Option<TargetUrls>, ResolveError> {
    if meta.kind != SpotifyKind::Album {
        let songs = search(http, meta, "song", country)?;
        let picked = best(meta, songs, |i| Candidate {
            title: i.track_name.as_deref().unwrap_or_default(),
            artist: &i.artist_name,
            duration_ms: i.track_time_millis,
            explicit: is_explicit(i.track_explicitness.as_deref()),
            ..Default::default()
        });
        return Ok(picked
            .and_then(|i| i.track_view_url)
            .and_then(|u| apple_music_urls(&u, true)));
    }

    // Album search leaves out some major albums, but song results carry their album,
    // so songs are the fallback.
    for entity in ["album", "song"] {
        let results = search(http, meta, entity, country)?;
        let picked = best(meta, results, |i| Candidate {
            title: i.collection_name.as_deref().unwrap_or_default(),
            artist: &i.artist_name,
            explicit: is_explicit(i.collection_explicitness.as_deref()),
            track_count: i.track_count,
            ..Default::default()
        });
        if let Some(url) = picked.and_then(|i| i.collection_view_url) {
            return Ok(apple_music_urls(&url, false));
        }
    }
    Ok(None)
}

fn search(
    http: &Http,
    meta: &SpotifyMeta,
    entity: &str,
    country: &str,
) -> Result<Vec<Item>, ResolveError> {
    let response: SearchResponse = json(
        http.get(SEARCH_URL)
            .query("term", &search_query(meta))
            .query("entity", entity)
            .query("country", country)
            .query("limit", "25"),
        "Apple Music",
    )?;
    Ok(response.results)
}

fn is_explicit(explicitness: Option<&str>) -> Option<bool> {
    match explicitness? {
        "explicit" => Some(true),
        "notExplicit" | "cleaned" => Some(false),
        _ => None,
    }
}
