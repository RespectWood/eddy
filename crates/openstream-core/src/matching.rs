//! Fuzzy matching between Spotify metadata and catalog search results.

use crate::spotify::SpotifyMeta;

/// Minimum score for a candidate to count as the same item.
const THRESHOLD: i32 = 70;

/// A search result from another catalog, reduced to what matching needs.
#[derive(Debug, Clone, Default)]
pub(crate) struct Candidate<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub duration_ms: Option<u64>,
    pub explicit: Option<bool>,
    pub track_count: Option<u32>,
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
    Some(score)
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
