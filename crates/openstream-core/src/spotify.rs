//! Spotify metadata, read from the public embed page (no API credentials needed).

use serde_json::Value;

use crate::error::ResolveError;
use crate::http::{map_err, Http, BROWSER_USER_AGENT};
use crate::link::{SpotifyKind, SpotifyLink};

#[derive(Debug, Clone, PartialEq)]
pub struct SpotifyMeta {
    pub kind: SpotifyKind,
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
    pub explicit: Option<bool>,
    /// Albums only.
    pub track_count: Option<u32>,
}

impl SpotifyMeta {
    pub fn primary_artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or_default()
    }

    /// "Artist A, Artist B"
    pub fn artist_line(&self) -> String {
        self.artists.join(", ")
    }
}

pub(crate) fn fetch(http: &Http, link: &SpotifyLink) -> Result<Option<SpotifyMeta>, ResolveError> {
    let url = format!(
        "https://open.spotify.com/embed/{}/{}",
        link.kind.as_str(),
        link.id
    );
    let html = http
        .get(&url)
        .set("User-Agent", BROWSER_USER_AGENT)
        .call()
        .map_err(|e| map_err("Spotify", e))?
        .into_string()
        .map_err(|e| ResolveError::Network(format!("Spotify: {e}")))?;
    Ok(parse_embed(&html, link.kind))
}

/// Extracts metadata from the `__NEXT_DATA__` JSON embedded in the page.
pub(crate) fn parse_embed(html: &str, kind: SpotifyKind) -> Option<SpotifyMeta> {
    let start_tag = html.find(r#"<script id="__NEXT_DATA__""#)?;
    let json_start = start_tag + html[start_tag..].find('>')? + 1;
    let json_end = json_start + html[json_start..].find("</script>")?;
    let data: Value = serde_json::from_str(&html[json_start..json_end]).ok()?;
    let entity = data.pointer("/props/pageProps/state/data/entity")?;

    let title = entity
        .get("title")
        .or_else(|| entity.get("name"))
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    let mut artists: Vec<String> = entity
        .get("artists")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|a| a.get("name").and_then(Value::as_str))
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_default();
    if artists.is_empty() {
        // Album embeds only have a display string such as "Artist A, Artist B".
        if let Some(subtitle) = entity.get("subtitle").and_then(Value::as_str) {
            artists = subtitle
                .split(", ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    if title.is_empty() || artists.is_empty() {
        return None;
    }
    Some(SpotifyMeta {
        kind,
        title,
        artists,
        duration_ms: entity.get("duration").and_then(Value::as_u64),
        explicit: entity.get("isExplicit").and_then(Value::as_bool),
        track_count: entity
            .get("trackList")
            .and_then(Value::as_array)
            .map(|t| t.len() as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_embed() {
        let html = include_str!("../tests/fixtures/spotify_embed_track.html");
        let meta = parse_embed(html, SpotifyKind::Track).unwrap();
        assert_eq!(meta.title, "Never Gonna Give You Up");
        assert_eq!(meta.artists, vec!["Rick Astley"]);
        assert_eq!(meta.duration_ms, Some(213573));
        assert_eq!(meta.explicit, Some(false));
    }

    #[test]
    fn parses_album_embed() {
        let html = include_str!("../tests/fixtures/spotify_embed_album.html");
        let meta = parse_embed(html, SpotifyKind::Album).unwrap();
        assert_eq!(meta.title, "The Dark Side of the Moon");
        assert_eq!(meta.artists, vec!["Pink Floyd"]);
        assert_eq!(meta.track_count, Some(10));
    }

    #[test]
    fn rejects_pages_without_data() {
        assert!(parse_embed("<html></html>", SpotifyKind::Track).is_none());
    }
}
