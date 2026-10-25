//! Runs the built `eddy` binary. Lookups answer from recorded responses
//! (`EDDY_REPLAY`, available in test builds only), so these tests are offline.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn eddy(args: &[&str], replay: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_eddy"));
    command
        .args(args)
        .env_remove("TIDAL_CLIENT_ID")
        .env_remove("TIDAL_CLIENT_SECRET");
    match replay {
        Some(path) => command.env("EDDY_REPLAY", path),
        None => command.env_remove("EDDY_REPLAY"),
    };
    command.output().expect("eddy runs")
}

fn stdout(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn cassette(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../eddy-core/tests/fixtures/cassettes")
        .join(format!("{name}.json"))
}

/// A cassette written for one test, deleted when dropped.
struct TempCassette(PathBuf);

impl TempCassette {
    fn new(name: &str, json: serde_json::Value) -> Self {
        let path =
            std::env::temp_dir().join(format!("eddy-cli-test-{}-{name}.json", std::process::id()));
        std::fs::write(&path, json.to_string()).unwrap();
        Self(path)
    }
}

impl Drop for TempCassette {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn parse_prints_json() {
    let output = eddy(
        &[
            "parse",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC?si=x",
        ],
        None,
    );
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        serde_json::json!({"type": "spotify", "kind": "track", "id": "4uLU6hMCjMI75M1A2tKUQC"})
    );
    let output = eddy(&["parse", "https://example.com"], None);
    assert_eq!(stdout(&output), serde_json::json!({"type": "not_music"}));
}

#[test]
fn usage_errors_exit_with_2() {
    for args in [
        &[][..],
        &["frobnicate"],
        &["parse"],
        &["resolve", "https://open.spotify.com/track/x"],
        &[
            "resolve",
            "https://open.spotify.com/track/x",
            "--to",
            "deezer",
        ],
        &["resolve", "https://open.spotify.com/track/x", "--country"],
        &["share", "--title", "Yeah!"],
        &[
            "share", "--title", "Yeah!", "--artist", "USHER", "--to", "deezer",
        ],
    ] {
        let output = eddy(args, None);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage:"),
            "{args:?}"
        );
    }
}

#[test]
fn resolve_prints_the_conversion() {
    let output = eddy(
        &[
            "resolve",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "--to",
            "apple-music",
            "--country",
            "DE",
        ],
        Some(&cassette("apple_music_track_rick_astley")),
    );
    assert!(output.status.success(), "{output:?}");
    let json = stdout(&output);
    assert_eq!(json["title"], "Never Gonna Give You Up");
    assert_eq!(json["service"], "apple-music");
    assert!(json["native_url"]
        .as_str()
        .unwrap()
        .starts_with("music://music.apple.com/de/"));
}

#[test]
fn resolve_without_a_match_exits_with_1() {
    let output = eddy(
        &[
            "resolve",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "--to",
            "tidal",
            "--country",
            "DE",
        ],
        Some(&cassette("tidal_without_credentials")),
    );
    assert_eq!(output.status.code(), Some(1));
    let json = stdout(&output);
    assert_eq!(json["error"], "not_found");
    assert_eq!(json["reason"], "no_credentials");
    assert!(json["search"]["native_url"]
        .as_str()
        .unwrap()
        .starts_with("tidal://search?q=Rick+Astley"));
}

#[test]
fn resolve_reports_errors() {
    let replay = TempCassette::new(
        "rate-limited",
        serde_json::json!({"exchanges": [{
            "request": "GET https://open.spotify.com/embed/track/4uLU6hMCjMI75M1A2tKUQC",
            "status": 429
        }]}),
    );
    let output = eddy(
        &[
            "resolve",
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC",
            "--to",
            "apple-music",
        ],
        Some(&replay.0),
    );
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output)["error"], "rate limited by Spotify");
}

#[test]
fn share_prints_the_link() {
    let output = eddy(
        &[
            "share",
            "--title",
            "Yeah! (feat. Lil Jon & Ludacris)",
            "--artist",
            "USHER",
            "--artist",
            "Lil Jon",
            "--artist",
            "Ludacris",
            "--duration",
            "4:10",
            "--to",
            "apple-music",
            "--country",
            "DE",
        ],
        Some(&cassette("share_usher_apple_music")),
    );
    assert!(output.status.success(), "{output:?}");
    let json = stdout(&output);
    assert_eq!(json["exact"], true);
    assert_eq!(json["target"], "apple-music");
    assert!(json["url"]
        .as_str()
        .unwrap()
        .starts_with("https://music.apple.com/de/album/"));
}
