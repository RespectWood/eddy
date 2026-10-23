//! Spotify links for a recording, from MusicBrainz's community-maintained URL
//! relationships (free, no key). Looked up by ISRC.
//!
//! MusicBrainz asks for at most one request per second and a User-Agent with
//! contact information: <https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting>.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::error::ResolveError;
use crate::http::{map_err, Http};
use crate::link::{parse, ParsedLink, SpotifyKind};

const API: &str = "https://musicbrainz.org/ws/2";
const USER_AGENT: &str = concat!(
    "openstream/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/RespectWood/openstream )"
);
const MIN_INTERVAL: Duration = Duration::from_millis(1100);

/// When the last request was sent, shared by every lookup in the process.
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, Default, Deserialize)]
struct IsrcResponse {
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Debug, Deserialize)]
struct Recording {
    #[serde(default)]
    relations: Vec<Relation>,
}

#[derive(Debug, Deserialize)]
struct Relation {
    url: Option<RelationUrl>,
}

#[derive(Debug, Deserialize)]
struct RelationUrl {
    resource: String,
}

/// Spotify track IDs linked to recordings with this ISRC, without duplicates, in
/// MusicBrainz's order. Empty when the ISRC or its links are unknown.
pub(crate) fn spotify_track_ids(http: &Http, isrc: &str) -> Result<Vec<String>, ResolveError> {
    let url = format!("{API}/isrc/{isrc}");
    let mut retried = false;
    let body = loop {
        throttle();
        let result = http
            .get(&url)
            .set("User-Agent", USER_AGENT)
            .query("inc", "url-rels")
            .query("fmt", "json")
            .call();
        match result {
            Ok(response) => {
                break response
                    .into_string()
                    .map_err(|e| ResolveError::Network(format!("MusicBrainz: {e}")))?
            }
            Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
            // "The web server is currently busy": common at peak times, retry once.
            Err(ureq::Error::Status(503, _)) if !retried => retried = true,
            Err(e) => return Err(map_err("MusicBrainz", e)),
        }
    };
    Ok(parse_track_ids(&body))
}

fn parse_track_ids(body: &str) -> Vec<String> {
    let response: IsrcResponse = serde_json::from_str(body).unwrap_or_default();
    let mut ids: Vec<String> = Vec::new();
    for relation in response.recordings.iter().flat_map(|r| &r.relations) {
        let Some(url) = &relation.url else { continue };
        if let ParsedLink::Spotify(link) = parse(&url.resource) {
            if link.kind == SpotifyKind::Track && !ids.contains(&link.id) {
                ids.push(link.id);
            }
        }
    }
    ids
}

fn throttle() {
    let mut last = LAST_REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(previous) = *last {
        let wait = MIN_INTERVAL.saturating_sub(previous.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    *last = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded `GET /ws/2/isrc/USAR10301423?inc=url-rels&fmt=json` (trimmed).
    #[test]
    fn extracts_spotify_tracks_from_recorded_response() {
        let body = include_str!("../tests/fixtures/musicbrainz_isrc_usher.json");
        assert_eq!(
            parse_track_ids(body),
            vec!["1M18Qdzl9xFVRmUm66zn34", "5rb9QrpfcKFHM1EUbSIurX"]
        );
    }

    #[test]
    fn ignores_unknown_or_broken_responses() {
        assert!(parse_track_ids("{}").is_empty());
        assert!(parse_track_ids("not json").is_empty());
    }

    /// Hits the real MusicBrainz API: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_finds_spotify_links_for_usher() {
        let ids = spotify_track_ids(&Http::new(), "USAR10301423").unwrap();
        assert!(
            ids.contains(&"1M18Qdzl9xFVRmUm66zn34".to_string()),
            "{ids:?}"
        );
    }
}
