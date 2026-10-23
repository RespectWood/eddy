use std::time::Instant;

use eddy_core::{is_music_link, parse, ParsedLink};

fn describe(parsed: &ParsedLink) -> String {
    match parsed {
        ParsedLink::Spotify(link) => format!("spotify {} {}", link.kind.as_str(), link.id),
        ParsedLink::SpotifyShortLink { .. } => "short".to_string(),
        ParsedLink::NotMusic => "not_music".to_string(),
    }
}

fn fixtures() -> Vec<(String, String)> {
    include_str!("fixtures/links.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (url, expected) = l.split_once('\t').expect("fixture lines are tab-separated");
            (url.to_string(), expected.to_string())
        })
        .collect()
}

#[test]
fn fixture_corpus() {
    for (url, expected) in fixtures() {
        assert_eq!(describe(&parse(&url)), expected, "{url}");
        assert_eq!(is_music_link(&url), expected != "not_music", "{url}");
    }
}

#[test]
fn parse_is_fast() {
    let urls: Vec<String> = fixtures().into_iter().map(|(u, _)| u).collect();
    let rounds = 2_000;
    let start = Instant::now();
    for _ in 0..rounds {
        for url in &urls {
            std::hint::black_box(parse(url));
        }
    }
    let per_call = start.elapsed() / (rounds * urls.len() as u32);
    // Typically a few µs (even in debug builds); 1 ms is the hard limit from the plan.
    assert!(
        per_call.as_micros() < 1_000,
        "parse took {per_call:?} per call"
    );
}
