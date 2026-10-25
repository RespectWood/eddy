//! Apple Music lookup through the iTunes Search API (no credentials needed).

use serde::Deserialize;

use crate::error::ResolveError;
use crate::explain::printable;
use crate::http::{Http, Request};
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
            printable(c.title),
            printable(c.artist),
            printable(item.collection_name.as_deref().unwrap_or_default()),
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
    let response: SearchResponse = http.json(
        &Request::get(SEARCH_URL)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::offline;

    fn usher() -> SpotifyMeta {
        SpotifyMeta {
            kind: SpotifyKind::Track,
            title: "Yeah! (feat. Lil Jon & Ludacris)".into(),
            artists: vec!["USHER".into(), "Lil Jon".into(), "Ludacris".into()],
            duration_ms: Some(250_373),
            explicit: Some(false),
            track_count: None,
        }
    }

    /// The recorded search (`apple_music_track_usher`) also has the song on two
    /// "Various Artists" compilations and a 20th anniversary edition.
    #[test]
    fn picks_the_original_album_from_recorded_results() {
        let (http, _) = offline("apple_music_track_usher");
        let found = find(&http, &usher(), "SE", false).unwrap().unwrap();
        assert!(
            found.urls.web_url.ends_with("?i=386153478"),
            "{}",
            found.urls.web_url
        );
        assert_eq!(
            found.album.as_deref(),
            Some("Confessions (Expanded Edition)")
        );
        assert_eq!(found.album_artist.as_deref(), Some("USHER"));
        assert!(found.artist_urls.is_none());
    }

    #[test]
    fn recognizes_compilations() {
        let (http, _) = offline("apple_music_track_usher");
        let results = search(&http, &usher(), "song", "SE").unwrap();
        let compilations: Vec<&str> = results
            .iter()
            .filter(|i| i.is_compilation())
            .filter_map(|i| i.collection_name.as_deref())
            .collect();
        assert!(
            compilations.contains(&"Pop Internacional 2023"),
            "{compilations:?}"
        );
        assert!(compilations.contains(&"Touch My Soul"), "{compilations:?}");
    }

    #[test]
    fn reads_explicitness() {
        assert_eq!(is_explicit(Some("explicit")), Some(true));
        assert_eq!(is_explicit(Some("cleaned")), Some(false));
        assert_eq!(is_explicit(Some("notExplicit")), Some(false));
        assert_eq!(is_explicit(Some("unknown")), None);
        assert_eq!(is_explicit(None), None);
    }

    /// Results without a link can't be opened, so they never win.
    #[test]
    fn ignores_results_without_a_link() {
        let (http, _) = crate::replay::replaying(crate::replay::Replay::new().with(
            crate::replay::Exchange::json(
                "GET https://itunes.apple.com/search",
                serde_json::json!({"results": [{
                    "trackName": "Yeah! (feat. Lil Jon & Ludacris)",
                    "artistName": "USHER",
                    "collectionName": "Confessions",
                    "trackTimeMillis": 250373
                }]}),
            ),
        ));
        assert!(find(&http, &usher(), "SE", false).unwrap().is_none());
    }
}
