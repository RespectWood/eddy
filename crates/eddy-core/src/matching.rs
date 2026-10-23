//! Fuzzy matching between Spotify metadata and catalog search results.

use crate::spotify::SpotifyMeta;

/// Minimum score for a candidate to count as the same item.
pub(crate) const THRESHOLD: i32 = 70;

/// A search result from another catalog, reduced to what matching needs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Candidate<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub duration_ms: Option<u64>,
    pub explicit: Option<bool>,
    pub track_count: Option<u32>,
    /// On a "Various Artists" compilation: still a match, but the original release is preferred.
    pub compilation: bool,
}

/// One release (album) that a recording or edition appears on. The same ISRC or UPC
/// often appears on many releases (originals, reissues, compilations), and the original
/// one should open.
#[derive(Debug, Clone, Default)]
pub(crate) struct Release<'a> {
    pub album_title: &'a str,
    pub album_artists: &'a [String],
    /// 0.0–1.0, as reported by the catalog.
    pub popularity: f64,
    pub release_year: Option<u32>,
    pub explicit: Option<bool>,
}

/// Lowercase, without bracketed parts, "- Remastered 2011"-style suffixes,
/// "feat." credits and punctuation.
pub(crate) fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut base = lower.as_str();
    if let Some(i) = base.find(" - ") {
        base = &base[..i];
    }
    for marker in [" feat. ", " feat ", " ft. ", " featuring "] {
        if let Some(i) = base.find(marker) {
            base = &base[..i];
        }
    }
    let mut out = String::with_capacity(base.len());
    let mut depth = 0u32;
    for c in base.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '&' => out.push_str(" and "),
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The query sent to catalog search APIs.
pub(crate) fn search_query(meta: &SpotifyMeta) -> String {
    format!("{} {}", meta.primary_artist(), normalize(&meta.title))
}

/// Scores how likely `candidate` is the same item as `meta`. `None` means "definitely not".
pub(crate) fn score(meta: &SpotifyMeta, candidate: &Candidate) -> Option<i32> {
    let wanted_title = normalize(&meta.title);
    let title = normalize(candidate.title);
    if wanted_title.is_empty() || title.is_empty() {
        return None;
    }
    let mut score = if title == wanted_title {
        50
    } else if title.contains(&wanted_title) || wanted_title.contains(&title) {
        30
    } else {
        return None;
    };
    if candidate
        .title
        .trim()
        .eq_ignore_ascii_case(meta.title.trim())
    {
        score += 10;
    }

    let artist = normalize(candidate.artist);
    let artist_matches = |a: &str| {
        let a = normalize(a);
        !a.is_empty() && (artist.contains(&a) || a.contains(&artist))
    };
    score += if artist_matches(meta.primary_artist()) {
        30
    } else if meta.artists.iter().skip(1).any(|a| artist_matches(a)) {
        20
    } else {
        return None;
    };

    if let (Some(a), Some(b)) = (meta.duration_ms, candidate.duration_ms) {
        score += match a.abs_diff(b) {
            0..=2_000 => 20,
            2_001..=5_000 => 10,
            5_001..=15_000 => 0,
            _ => -30,
        };
    }
    if let (Some(a), Some(b)) = (meta.track_count, candidate.track_count) {
        score += if a == b { 20 } else { -10 };
    }
    if let (Some(a), Some(b)) = (meta.explicit, candidate.explicit) {
        score += if a == b { 5 } else { -5 };
    }
    if candidate.compilation {
        score -= 25;
    }
    Some(score)
}

pub(crate) fn is_match(meta: &SpotifyMeta, candidate: &Candidate) -> bool {
    score(meta, candidate).is_some_and(|s| s >= THRESHOLD)
}

pub(crate) fn is_various_artists(name: &str) -> bool {
    matches!(
        normalize(name).as_str(),
        "various artists"
            | "various"
            | "verschiedene interpreten"
            | "artistes divers"
            | "varios artistas"
    )
}

/// Scores how likely `release` is the original release of `meta`. `album_hint` is
/// the album the item was matched on elsewhere (Deezer), empty if unknown.
pub(crate) fn release_score(meta: &SpotifyMeta, album_hint: &str, release: &Release) -> f64 {
    let mut score = 0.0;
    let primary = normalize(meta.primary_artist());
    let by_artist = release.album_artists.iter().any(|a| {
        let a = normalize(a);
        !a.is_empty() && !primary.is_empty() && (a.contains(&primary) || primary.contains(&a))
    });
    if release.album_artists.iter().any(|a| is_various_artists(a)) {
        score -= 100.0;
    } else if by_artist {
        score += 100.0;
    }
    score += release.popularity.clamp(0.0, 1.0) * 50.0;
    let hint = normalize(album_hint);
    if !hint.is_empty() {
        let title = normalize(release.album_title);
        if release
            .album_title
            .trim()
            .eq_ignore_ascii_case(album_hint.trim())
        {
            score += 40.0;
        } else if title == hint {
            score += 30.0;
        } else if !title.is_empty() && (title.contains(&hint) || hint.contains(&title)) {
            score += 15.0;
        }
    }
    // Older releases are slightly preferred: the original usually came first.
    if let Some(year) = release.release_year {
        score += f64::from(2030u32.saturating_sub(year)) * 0.2;
    }
    if let (Some(a), Some(b)) = (meta.explicit, release.explicit) {
        score += if a == b { 5.0 } else { -5.0 };
    }
    score
}

/// Picks the release most likely to be the original. The first item wins ties.
pub(crate) fn best_release<T>(
    meta: &SpotifyMeta,
    album_hint: &str,
    items: Vec<T>,
    release: impl Fn(&T) -> Release,
) -> Option<T> {
    let mut best: Option<(f64, T)> = None;
    for item in items {
        let s = release_score(meta, album_hint, &release(&item));
        if best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, item));
        }
    }
    best.map(|(_, item)| item)
}

/// Picks the best-scoring item above the threshold.
pub(crate) fn best<T>(
    meta: &SpotifyMeta,
    items: Vec<T>,
    candidate: impl Fn(&T) -> Candidate,
) -> Option<T> {
    let mut best: Option<(i32, T)> = None;
    for item in items {
        let Some(s) = score(meta, &candidate(&item)) else {
            continue;
        };
        // Strictly greater, so the API's earlier (more relevant) result wins ties.
        if s >= THRESHOLD && best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, item));
        }
    }
    best.map(|(_, item)| item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::SpotifyKind;

    fn meta(title: &str, artists: &[&str], duration_ms: u64) -> SpotifyMeta {
        SpotifyMeta {
            kind: SpotifyKind::Track,
            title: title.to_string(),
            artists: artists.iter().map(|s| s.to_string()).collect(),
            duration_ms: Some(duration_ms),
            explicit: Some(false),
            track_count: None,
        }
    }

    #[test]
    fn normalizes_titles() {
        assert_eq!(
            normalize("Wish You Were Here - Remastered 2011"),
            "wish you were here"
        );
        assert_eq!(normalize("Stay (feat. Justin Bieber)"), "stay");
        assert_eq!(normalize("Song feat. Someone"), "song");
        assert_eq!(normalize("Simon & Garfunkel"), "simon and garfunkel");
        assert_eq!(normalize("Sigur Rós"), "sigur rós");
        assert_eq!(normalize("  Hello,   World!! "), "hello world");
    }

    #[test]
    fn prefers_matching_duration_and_exact_title() {
        let m = meta("Never Gonna Give You Up", &["Rick Astley"], 213_573);
        let items = vec![
            (
                "Never Gonna Give You Up (Extended Mix)",
                "Rick Astley",
                340_000u64,
            ),
            ("Never Gonna Give You Up", "Rick Astley", 214_000),
            ("Never Gonna Give You Up", "Some Cover Band", 213_000),
        ];
        let picked = best(&m, items, |(t, a, d)| Candidate {
            title: t,
            artist: a,
            duration_ms: Some(*d),
            ..Default::default()
        });
        assert_eq!(
            picked,
            Some(("Never Gonna Give You Up", "Rick Astley", 214_000))
        );
    }

    #[test]
    fn rejects_wrong_artist_or_title() {
        let m = meta("Hello", &["Adele"], 295_000);
        let wrong_artist = Candidate {
            title: "Hello",
            artist: "Lionel Richie",
            duration_ms: Some(295_000),
            ..Default::default()
        };
        let wrong_title = Candidate {
            title: "Someone Like You",
            artist: "Adele",
            duration_ms: Some(295_000),
            ..Default::default()
        };
        assert!(score(&m, &wrong_artist).is_none());
        assert!(score(&m, &wrong_title).is_none());
    }

    #[test]
    fn matches_featured_artist() {
        let m = meta("Under Pressure", &["Queen", "David Bowie"], 248_000);
        let c = Candidate {
            title: "Under Pressure",
            artist: "David Bowie",
            duration_ms: Some(248_500),
            ..Default::default()
        };
        assert!(score(&m, &c).unwrap() >= THRESHOLD);
    }

    #[test]
    fn prefers_original_release_over_compilations() {
        let m = meta(
            "Yeah! (feat. Lil Jon & Ludacris)",
            &["USHER", "Lil Jon", "Ludacris"],
            250_373,
        );
        let usher = vec!["Usher".to_string()];
        let various = vec!["Various Artists".to_string()];
        let releases = vec![
            ("Summer Party Hits", &various, 0.34, 2024),
            ("Beach Party Anthems", &various, 0.44, 2023),
            ("Confessions (20th Anniversary Edition)", &usher, 0.16, 2004),
            ("Confessions (Expanded Edition)", &usher, 0.88, 2004),
        ];
        let picked = best_release(
            &m,
            "Confessions (Expanded Edition)",
            releases,
            |(t, a, p, y)| Release {
                album_title: t,
                album_artists: a,
                popularity: *p,
                release_year: Some(*y),
                explicit: None,
            },
        );
        assert_eq!(picked.unwrap().0, "Confessions (Expanded Edition)");
    }

    #[test]
    fn original_wins_without_album_hint() {
        let m = meta("Hello", &["Adele"], 295_000);
        let adele = vec!["Adele".to_string()];
        let various = vec!["Various Artists".to_string()];
        let releases = vec![("Now 92", &various, 0.9, 2015), ("25", &adele, 0.5, 2015)];
        let picked = best_release(&m, "", releases, |(t, a, p, y)| Release {
            album_title: t,
            album_artists: a,
            popularity: *p,
            release_year: Some(*y),
            explicit: None,
        });
        assert_eq!(picked.unwrap().0, "25");
    }

    #[test]
    fn compilation_candidates_lose_to_originals() {
        let m = meta("Hello", &["Adele"], 295_000);
        let original = Candidate {
            title: "Hello",
            artist: "Adele",
            duration_ms: Some(295_000),
            ..Default::default()
        };
        let compilation = Candidate {
            compilation: true,
            ..original.clone()
        };
        let (o, c) = (
            score(&m, &original).unwrap(),
            score(&m, &compilation).unwrap(),
        );
        assert!(o > c && c >= THRESHOLD, "{o} {c}");
        assert!(
            is_various_artists("Various Artists") && is_various_artists("Verschiedene Interpreten")
        );
        assert!(!is_various_artists("Usher"));
    }

    #[test]
    fn far_off_duration_falls_below_threshold() {
        let m = meta("Intro", &["The xx"], 127_000);
        let c = Candidate {
            title: "Intro",
            artist: "The xx",
            duration_ms: Some(400_000),
            ..Default::default()
        };
        assert!(score(&m, &c).unwrap() < THRESHOLD);
    }
}
