//! Apple Music lookup through the iTunes Search API (no credentials needed).

use serde::Deserialize;

use crate::error::ResolveError;
use crate::http::{json, Http};
use crate::link::SpotifyKind;
use crate::matching::{best, is_various_artists, score, search_query, Candidate};
use crate::resolve::Found;
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
    /// Only present when it differs from `artist_name`, e.g. "Various Artists" on compilations.
    collection_artist_name: Option<String>,
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
    explain: bool,
) -> Result<Option<Found>, ResolveError> {
    if meta.kind != SpotifyKind::Album {
        let songs = search(http, meta, "song", country)?;
        if explain {
            explain_items(meta, "song", &songs, Item::song_candidate);
        }
        let picked = best(meta, songs, Item::song_candidate);
        return Ok(picked.and_then(|i| {
            let urls = apple_music_urls(i.track_view_url.as_deref()?, true)?;
            Some(i.found(urls))
        }));
    }

    // Album search leaves out some major albums, but song results carry their album,
    // so songs are the fallback.
    for entity in ["album", "song"] {
        let results = search(http, meta, entity, country)?;
        if explain {
            explain_items(meta, entity, &results, Item::album_candidate);
        }
        let picked = best(meta, results, Item::album_candidate);
        if let Some(found) = picked.and_then(|i| {
            let urls = apple_music_urls(i.collection_view_url.as_deref()?, false)?;
            Some(i.found(urls))
        }) {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn explain_items(
    meta: &SpotifyMeta,
    entity: &str,
    items: &[Item],
    candidate: fn(&Item) -> Candidate,
) {
    eprintln!("Apple Music {entity} candidates:");
    for item in items {
        let c = candidate(item);
        let score = score(meta, &c).map_or_else(|| "    -".to_string(), |s| format!("{s:>5}"));
        eprintln!(
            "  {score}  {} — {} [{}]{}",
            c.title,
            c.artist,
            item.collection_name.as_deref().unwrap_or_default(),
            if c.compilation { " (compilation)" } else { "" }
        );
    }
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

impl Item {
    fn is_compilation(&self) -> bool {
        self.collection_artist_name
            .as_deref()
            .is_some_and(is_various_artists)
    }

    fn song_candidate(&self) -> Candidate<'_> {
        Candidate {
            title: self.track_name.as_deref().unwrap_or_default(),
            artist: &self.artist_name,
            duration_ms: self.track_time_millis,
            explicit: is_explicit(self.track_explicitness.as_deref()),
            compilation: self.is_compilation(),
            ..Default::default()
        }
    }

    fn album_candidate(&self) -> Candidate<'_> {
        Candidate {
            title: self.collection_name.as_deref().unwrap_or_default(),
            artist: &self.artist_name,
            explicit: is_explicit(self.collection_explicitness.as_deref()),
            track_count: self.track_count,
            compilation: self.is_compilation(),
            ..Default::default()
        }
    }

    fn found(self, urls: TargetUrls) -> Found {
        Found {
            urls,
            artist_urls: None,
            album: self.collection_name,
            album_artist: self.collection_artist_name.or(Some(self.artist_name)),
        }
    }
}

fn is_explicit(explicitness: Option<&str>) -> Option<bool> {
    match explicitness? {
        "explicit" => Some(true),
        "notExplicit" | "cleaned" => Some(false),
        _ => None,
    }
}
