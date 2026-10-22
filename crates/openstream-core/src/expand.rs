//! Short link expansion (`spotify.link`, `spoti.fi`).

use crate::error::ResolveError;
use crate::http::{map_err, Http};
use crate::link::{parse, ParsedLink, SpotifyLink};

/// Follows a short link to the Spotify content it points to.
///
/// Short links usually answer with a redirect, but some (branch.io) answer with an
/// HTML page that contains the target URL, so the body is searched as a fallback.
pub(crate) fn expand(http: &Http, short_url: &str) -> Result<Option<SpotifyLink>, ResolveError> {
    let response = http
        .get(short_url)
        .call()
        .map_err(|e| map_err("Spotify short link", e))?;
    if let ParsedLink::Spotify(link) = parse(response.get_url()) {
        return Ok(Some(link));
    }
    let body = response
        .into_string()
        .map_err(|e| ResolveError::Network(format!("Spotify short link: {e}")))?;
    Ok(find_link_in_body(&body))
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
    fn no_link_in_html() {
        assert!(find_link_in_body("<html>nothing</html>").is_none());
    }
}
