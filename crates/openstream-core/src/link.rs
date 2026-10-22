//! Spotify link parsing.
//!
//! This runs for every link the user clicks, so it must stay synchronous and
//! free of I/O. Anything that needs the network (short link expansion,
//! resolving) lives in other modules.

use serde::{Deserialize, Serialize};
use url::Url;

/// Hosts that serve Spotify content pages.
const CONTENT_HOSTS: &[&str] = &["open.spotify.com", "play.spotify.com"];

/// Hosts that serve Spotify short links, which redirect to a content page.
const SHORT_LINK_HOSTS: &[&str] = &["spotify.link", "spoti.fi", "spotify.app.link"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpotifyKind {
    Track,
    Album,
    Artist,
    Playlist,
    Episode,
    Show,
}

impl SpotifyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Artist => "artist",
            Self::Playlist => "playlist",
            Self::Episode => "episode",
            Self::Show => "show",
        }
    }

    fn from_segment(segment: &str) -> Option<Self> {
        Some(match segment.to_ascii_lowercase().as_str() {
            "track" => Self::Track,
            "album" => Self::Album,
            "artist" => Self::Artist,
            "playlist" => Self::Playlist,
            "episode" => Self::Episode,
            "show" => Self::Show,
            _ => return None,
        })
    }

    /// Whether openstream converts this kind to another service (v0.1: tracks and albums).
    pub fn is_convertible(self) -> bool {
        matches!(self, Self::Track | Self::Album)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SpotifyLink {
    pub kind: SpotifyKind,
    pub id: String,
}

impl SpotifyLink {
    pub fn canonical_url(&self) -> String {
        format!(
            "https://open.spotify.com/{}/{}",
            self.kind.as_str(),
            self.id
        )
    }

    pub fn uri(&self) -> String {
        format!("spotify:{}:{}", self.kind.as_str(), self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParsedLink {
    Spotify(SpotifyLink),
    /// A short link that must be expanded over the network before it can be parsed.
    SpotifyShortLink {
        url: String,
    },
    NotMusic,
}

/// Parses a clicked URL. Sync and free of I/O.
pub fn parse(input: &str) -> ParsedLink {
    let input = input.trim();
    if starts_with_ignore_case(input, "spotify:") {
        return parse_uri(input);
    }
    let Ok(url) = Url::parse(input) else {
        return ParsedLink::NotMusic;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return ParsedLink::NotMusic;
    }
    // `Url` lowercases the host for us.
    let Some(host) = url.host_str() else {
        return ParsedLink::NotMusic;
    };
    if SHORT_LINK_HOSTS.contains(&host) {
        let has_code = url
            .path_segments()
            .is_some_and(|mut s| s.any(|seg| !seg.is_empty()));
        return if has_code {
            ParsedLink::SpotifyShortLink {
                url: input.to_string(),
            }
        } else {
            ParsedLink::NotMusic
        };
    }
    if !CONTENT_HOSTS.contains(&host) {
        return ParsedLink::NotMusic;
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();
    parse_path(&segments)
}

/// Whether the URL is something openstream should look at (any Spotify link).
pub fn is_music_link(input: &str) -> bool {
    !matches!(parse(input), ParsedLink::NotMusic)
}

fn parse_path(segments: &[&str]) -> ParsedLink {
    let mut rest = segments;
    // Locale prefix, e.g. `/intl-de/track/…` or `/intl-pt-BR/track/…`.
    if rest.first().is_some_and(|s| is_locale_segment(s)) {
        rest = &rest[1..];
    }
    // Embed pages, e.g. `/embed/track/…`.
    if rest
        .first()
        .is_some_and(|s| s.eq_ignore_ascii_case("embed"))
    {
        rest = &rest[1..];
    }
    // Old playlist form: `/user/{user}/playlist/{id}`.
    if rest.len() == 4 && rest[0].eq_ignore_ascii_case("user") {
        rest = &rest[2..];
    }
    match rest {
        [kind, id] => link_from(kind, id),
        _ => ParsedLink::NotMusic,
    }
}

fn parse_uri(input: &str) -> ParsedLink {
    let parts: Vec<&str> = input.split(':').collect();
    match parts.as_slice() {
        [_, kind, id] => link_from(kind, id),
        [_, user, _, kind, id] if user.eq_ignore_ascii_case("user") => link_from(kind, id),
        _ => ParsedLink::NotMusic,
    }
}

fn link_from(kind: &str, id: &str) -> ParsedLink {
    match SpotifyKind::from_segment(kind) {
        Some(kind) if is_valid_id(id) => ParsedLink::Spotify(SpotifyLink {
            kind,
            id: id.to_string(),
        }),
        _ => ParsedLink::NotMusic,
    }
}

/// Spotify IDs are 22 base62 characters.
fn is_valid_id(id: &str) -> bool {
    id.len() == 22 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `intl-xx` or `intl-xx-YY`.
fn is_locale_segment(segment: &str) -> bool {
    let Some(code) = segment.strip_prefix("intl-") else {
        return false;
    };
    let mut parts = code.split('-');
    let lang_ok = parts
        .next()
        .is_some_and(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()));
    let region_ok = parts
        .next()
        .is_none_or(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic()));
    lang_ok && region_ok && parts.next().is_none()
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "4uLU6hMCjMI75M1A2tKUQC";

    fn spotify(kind: SpotifyKind) -> ParsedLink {
        ParsedLink::Spotify(SpotifyLink {
            kind,
            id: ID.to_string(),
        })
    }

    #[test]
    fn parses_content_links() {
        let cases = [
            (
                format!("https://open.spotify.com/track/{ID}"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/album/{ID}"),
                spotify(SpotifyKind::Album),
            ),
            (
                format!("https://open.spotify.com/artist/{ID}"),
                spotify(SpotifyKind::Artist),
            ),
            (
                format!("https://open.spotify.com/playlist/{ID}"),
                spotify(SpotifyKind::Playlist),
            ),
            (
                format!("https://open.spotify.com/episode/{ID}"),
                spotify(SpotifyKind::Episode),
            ),
            (
                format!("https://open.spotify.com/show/{ID}"),
                spotify(SpotifyKind::Show),
            ),
            (
                format!("https://open.spotify.com/track/{ID}?si=abc123&context=x"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/track/{ID}#fragment"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/track/{ID}/"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("http://open.spotify.com/track/{ID}"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://OPEN.SPOTIFY.COM/track/{ID}"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://play.spotify.com/track/{ID}"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/intl-de/track/{ID}?si=x"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/intl-pt-BR/album/{ID}"),
                spotify(SpotifyKind::Album),
            ),
            (
                format!("https://open.spotify.com/embed/track/{ID}"),
                spotify(SpotifyKind::Track),
            ),
            (
                format!("https://open.spotify.com/user/someone/playlist/{ID}"),
                spotify(SpotifyKind::Playlist),
            ),
            (
                format!("  https://open.spotify.com/track/{ID}  "),
                spotify(SpotifyKind::Track),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(parse(&input), expected, "{input}");
        }
    }

    #[test]
    fn parses_uris() {
        assert_eq!(
            parse(&format!("spotify:track:{ID}")),
            spotify(SpotifyKind::Track)
        );
        assert_eq!(
            parse(&format!("SPOTIFY:album:{ID}")),
            spotify(SpotifyKind::Album)
        );
        assert_eq!(
            parse(&format!("spotify:user:someone:playlist:{ID}")),
            spotify(SpotifyKind::Playlist)
        );
        assert_eq!(parse("spotify:track:short"), ParsedLink::NotMusic);
        assert_eq!(parse("spotify:"), ParsedLink::NotMusic);
    }

    #[test]
    fn detects_short_links() {
        for input in [
            "https://spotify.link/AbCdEf",
            "https://spoti.fi/3abc",
            "https://spotify.app.link/xyz?_p=1",
        ] {
            assert_eq!(
                parse(input),
                ParsedLink::SpotifyShortLink {
                    url: input.to_string()
                },
                "{input}"
            );
        }
        assert_eq!(parse("https://spotify.link/"), ParsedLink::NotMusic);
    }

    #[test]
    fn rejects_everything_else() {
        let cases = [
            "https://example.com".to_string(),
            "https://open.spotify.com/".to_string(),
            "https://open.spotify.com/user/someone".to_string(),
            "https://open.spotify.com/search/rick".to_string(),
            "https://open.spotify.com/track/".to_string(),
            "https://open.spotify.com/track/tooShort".to_string(),
            format!("https://open.spotify.com/track/{ID}x"),
            format!("https://open.spotify.com/track/{}!", &ID[..21]),
            format!("https://open.spotify.com/podcast/{ID}"),
            format!("https://open.spotify.com/intl-deutsch/track/{ID}"),
            format!("https://open.spotify.com.evil.tld/track/{ID}"),
            format!("https://evil.tld/open.spotify.com/track/{ID}"),
            format!("https://notopen.spotify.com/track/{ID}"),
            format!("ftp://open.spotify.com/track/{ID}"),
            "mailto:someone@example.com".to_string(),
            "not a url".to_string(),
            String::new(),
        ];
        for input in cases {
            assert_eq!(parse(&input), ParsedLink::NotMusic, "{input}");
        }
    }

    #[test]
    fn builds_canonical_forms() {
        let link = SpotifyLink {
            kind: SpotifyKind::Album,
            id: ID.to_string(),
        };
        assert_eq!(
            link.canonical_url(),
            format!("https://open.spotify.com/album/{ID}")
        );
        assert_eq!(link.uri(), format!("spotify:album:{ID}"));
    }

    #[test]
    fn serializes_to_expected_json() {
        let json = serde_json::to_string(&parse(&format!(
            "https://open.spotify.com/intl-de/track/{ID}?si=abc"
        )))
        .unwrap();
        assert_eq!(
            json,
            format!(r#"{{"type":"spotify","kind":"track","id":"{ID}"}}"#)
        );
        assert_eq!(
            serde_json::to_string(&ParsedLink::NotMusic).unwrap(),
            r#"{"type":"not_music"}"#
        );
    }
}
