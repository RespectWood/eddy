//! Short link expansion (`spotify.link`, `spoti.fi`).

use url::Url;

use crate::error::ResolveError;
use crate::http::{map_err, Http};
use crate::link::{parse, ParsedLink, SpotifyLink};

const MAX_REDIRECTS: usize = 5;

/// Follows a short link to the Spotify content it points to.
///
/// Short links usually answer with a redirect, but some (branch.io) answer with an
/// HTML page that contains the target URL, so the body is searched as a fallback.
///
/// Every hop is fetched over HTTPS, even when the link or a redirect says `http://`,
/// so nobody on the network can swap in another song.
pub(crate) fn expand(http: &Http, short_url: &str) -> Result<Option<SpotifyLink>, ResolveError> {
    let network = |message: String| ResolveError::Network(format!("Spotify short link: {message}"));
    let mut url = https(short_url).ok_or_else(|| network("not a web link".into()))?;
    for _ in 0..=MAX_REDIRECTS {
        let response = http
            .get_without_redirects(url.as_str())
            .call()
            .map_err(|e| map_err("Spotify short link", e))?;
        if !(300..400).contains(&response.status()) {
            let body = response.into_string().map_err(|e| network(e.to_string()))?;
            return Ok(find_link_in_body(&body));
        }
        let location = response
            .header("Location")
            .ok_or_else(|| network("redirect without a target".into()))?;
        let next = url.join(location).map_err(|e| network(e.to_string()))?;
        // The content link itself needn't be fetched: it's all we were after.
        if let ParsedLink::Spotify(link) = parse(next.as_str()) {
            return Ok(Some(link));
        }
        url = https(next.as_str()).ok_or_else(|| network("redirect to a non-web link".into()))?;
    }
    Err(network("too many redirects".into()))
}

/// `url` with `http` upgraded to `https`; `None` for any other scheme.
fn https(url: &str) -> Option<Url> {
    let mut url = Url::parse(url).ok()?;
    match url.scheme() {
        "https" => {}
        "http" => url.set_scheme("https").ok()?,
        _ => return None,
    }
    Some(url)
}

pub(crate) fn find_link_in_body(body: &str) -> Option<SpotifyLink> {
    let body = body.replace("\\/", "/");
    let marker = "open.spotify.com/";
    let mut rest = body.as_str();
    while let Some(pos) = rest.find(marker) {
        let candidate = &rest[pos..];
        let end = candidate
            .find(|c: char| {
                matches!(c, '"' | '\'' | '<' | '>' | '\\' | '?' | '&' | '#') || c.is_whitespace()
            })
            .unwrap_or(candidate.len());
        if let ParsedLink::Spotify(link) = parse(&format!("https://{}", &candidate[..end])) {
            return Some(link);
        }
        rest = &candidate[marker.len()..];
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::SpotifyKind;

    #[test]
    fn finds_link_in_html() {
        let body = r#"<html><a href="https://open.spotify.com/">home</a>
            <script>var data = {"$canonical_url":"https:\/\/open.spotify.com\/track\/4uLU6hMCjMI75M1A2tKUQC?si=x"}</script>"#;
        let link = find_link_in_body(body).unwrap();
        assert_eq!(link.kind, SpotifyKind::Track);
        assert_eq!(link.id, "4uLU6hMCjMI75M1A2tKUQC");
    }

    #[test]
    fn fetches_over_https_only() {
        assert_eq!(
            https("http://spotify.link/abc").unwrap().as_str(),
            "https://spotify.link/abc"
        );
        assert_eq!(
            https("https://spoti.fi/abc?x=1").unwrap().as_str(),
            "https://spoti.fi/abc?x=1"
        );
        assert!(https("ftp://spotify.link/abc").is_none());
        assert!(https("file:///etc/passwd").is_none());
    }

    #[test]
    fn no_link_in_html() {
        assert!(find_link_in_body("<html>nothing</html>").is_none());
    }
}
