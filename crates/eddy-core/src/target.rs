//! Target services and the URLs used to open items in them.
//!
//! Every target item has two URLs: a `native_url` that opens the desktop app
//! (custom scheme, never `https`, so it can't loop back to Eddy as the
//! default browser) and a `web_url` used only when the app isn't installed.

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Service {
    AppleMusic,
    Tidal,
}

impl Service {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppleMusic => "apple-music",
            Self::Tidal => "tidal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "apple-music" | "applemusic" | "apple" | "music" => Some(Self::AppleMusic),
            "tidal" => Some(Self::Tidal),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetUrls {
    pub native_url: String,
    pub web_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Track,
    Album,
    Artist,
}

impl ItemKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
            Self::Artist => "artist",
        }
    }
}

/// Builds URLs from an Apple Music / iTunes web URL such as
/// `https://music.apple.com/de/album/name/1559523357?i=1559523359&uo=4`.
/// Tracking parameters are dropped. `i` (the track within the album) is kept only
/// with `keep_track`, so album lookups that went through a song open the album.
pub fn apple_music_urls(web_url: &str, keep_track: bool) -> Option<TargetUrls> {
    let mut url = Url::parse(web_url).ok()?;
    if !url.host_str()?.ends_with("music.apple.com") {
        return None;
    }
    url.set_host(Some("music.apple.com")).ok()?;
    let track = url
        .query_pairs()
        .find(|(k, _)| keep_track && k == "i")
        .map(|(_, v)| v.into_owned());
    url.set_query(None);
    url.set_fragment(None);
    if let Some(track) = track {
        url.query_pairs_mut().append_pair("i", &track);
    }
    let web_url = url.to_string();
    let native_url = format!("music://{}", web_url.trim_start_matches("https://"));
    Some(TargetUrls {
        native_url,
        web_url,
    })
}

pub fn tidal_urls(kind: ItemKind, id: &str) -> TargetUrls {
    TargetUrls {
        native_url: format!("tidal://{}/{id}", kind.as_str()),
        web_url: format!("https://tidal.com/browse/{}/{id}", kind.as_str()),
    }
}

/// URLs that open a search for `query` in the service. Used when there is no exact match.
pub fn search_urls(service: Service, query: &str, country: &str) -> TargetUrls {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let encoded = encoded.replace('+', "%20");
    match service {
        Service::AppleMusic => {
            let path = format!(
                "music.apple.com/{}/search?term={encoded}",
                country.to_ascii_lowercase()
            );
            TargetUrls {
                native_url: format!("music://{path}"),
                web_url: format!("https://{path}"),
            }
        }
        Service::Tidal => TargetUrls {
            native_url: format!("tidal://search?q={}", tidal_app_query(query)),
            web_url: format!("https://listen.tidal.com/search?q={encoded}"),
        },
    }
}

/// The Tidal desktop app runs `encodeURI` over the deep link before navigating, so any
/// `%` escape would be encoded twice and end up as literal text in the search box.
/// The query is therefore reduced to ASCII letters and digits (accents transliterated:
/// "Sigur Rós" → "Sigur Ros") joined with `+`, which the search page decodes as spaces.
fn tidal_app_query(query: &str) -> String {
    deunicode::deunicode(query)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_music_strips_tracking_and_keeps_track() {
        let urls = apple_music_urls(
            "https://music.apple.com/de/album/never-gonna-give-you-up/1559523357?i=1559523359&uo=4",
            true,
        )
        .unwrap();
        assert_eq!(
            urls.web_url,
            "https://music.apple.com/de/album/never-gonna-give-you-up/1559523357?i=1559523359"
        );
        assert_eq!(
            urls.native_url,
            "music://music.apple.com/de/album/never-gonna-give-you-up/1559523357?i=1559523359"
        );
    }

    #[test]
    fn apple_music_album_and_geo_host() {
        let urls = apple_music_urls(
            "https://geo.music.apple.com/us/album/_/1440857781?i=99&mt=1",
            false,
        )
        .unwrap();
        assert_eq!(
            urls.web_url,
            "https://music.apple.com/us/album/_/1440857781"
        );
        assert_eq!(
            urls.native_url,
            "music://music.apple.com/us/album/_/1440857781"
        );
        assert!(apple_music_urls("https://example.com/album/1", true).is_none());
    }

    #[test]
    fn tidal_deep_links() {
        let urls = tidal_urls(ItemKind::Track, "1234");
        assert_eq!(urls.native_url, "tidal://track/1234");
        assert_eq!(urls.web_url, "https://tidal.com/browse/track/1234");
        assert_eq!(
            tidal_urls(ItemKind::Album, "9").native_url,
            "tidal://album/9"
        );
    }

    #[test]
    fn search_urls_are_encoded() {
        let am = search_urls(Service::AppleMusic, "Rick Astley Never & Ever", "DE");
        assert_eq!(
            am.native_url,
            "music://music.apple.com/de/search?term=Rick%20Astley%20Never%20%26%20Ever"
        );
        let tidal = search_urls(Service::Tidal, "Sigur Rós Hoppípolla", "DE");
        assert_eq!(tidal.native_url, "tidal://search?q=Sigur+Ros+Hoppipolla");
        assert_eq!(
            tidal.web_url,
            "https://listen.tidal.com/search?q=Sigur%20R%C3%B3s%20Hopp%C3%ADpolla"
        );
    }
}
