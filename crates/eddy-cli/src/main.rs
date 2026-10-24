//! `Eddy` command-line tool, for development and testing.
//!
//! ```text
//! Eddy parse <url>
//! Eddy resolve <url> --to tidal|apple-music [--country DE] [--cache <path>] [--explain]
//! Eddy share --title <title> --artist <artist>… [--duration 4:10] [--to spotify|tidal|apple-music] [--country DE] [--explain]
//! ```
//!
//! `share` finds the link for a song played elsewhere ("Copy Link for Current Song");
//! Spotify unless `--to` says otherwise.
//!
//! `--explain` prints every candidate and its score to stderr.
//!
//! Tidal credentials are read from `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET`.

use std::path::PathBuf;
use std::process::ExitCode;

use eddy_core::{
    parse, NowPlaying, ResolveError, Resolver, ResolverConfig, Service, ShareTarget, TidalAuth,
    TidalCredentials,
};
use serde_json::json;

const USAGE: &str = "usage:
  eddy parse <url>
  eddy resolve <url> --to tidal|apple-music [--country DE] [--cache <path>] [--explain]
  eddy share --title <title> --artist <artist>… [--duration 4:10] [--to spotify|tidal|apple-music] [--country DE] [--explain]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("parse") if args.len() == 2 => {
            println!(
                "{}",
                serde_json::to_string(&parse(&args[1])).expect("serializable")
            );
            ExitCode::SUCCESS
        }
        Some("resolve") if args.len() >= 2 => resolve(&args[1], &args[2..]),
        Some("share") => share(&args[1..]),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn resolve(url: &str, options: &[String]) -> ExitCode {
    let mut service = None;
    let mut country = "US".to_string();
    let mut cache_path = None;
    let mut explain = false;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        if option == "--explain" {
            explain = true;
            continue;
        }
        let value = options.next();
        match (option.as_str(), value) {
            ("--to", Some(v)) => service = Service::parse(v),
            ("--country", Some(v)) => country = v.clone(),
            ("--cache", Some(v)) => cache_path = Some(PathBuf::from(v)),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(service) = service else {
        eprintln!("--to must be tidal or apple-music\n{USAGE}");
        return ExitCode::from(2);
    };
    let resolver = Resolver::new(ResolverConfig {
        cache_path,
        tidal: tidal_from_env(),
        explain,
    });
    match resolver.resolve(url, service, &country) {
        Ok(conversion) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&conversion).expect("serializable")
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            let details = match &error {
                ResolveError::NotFound {
                    reason,
                    title,
                    artist,
                    search,
                } => json!({
                    "error": "not_found",
                    "reason": reason,
                    "title": title,
                    "artist": artist,
                    "search": search,
                }),
                other => json!({ "error": other.to_string() }),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&details).expect("serializable")
            );
            ExitCode::FAILURE
        }
    }
}

/// Developer credentials from `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET`, if both are set.
fn tidal_from_env() -> Option<TidalAuth> {
    match (
        std::env::var("TIDAL_CLIENT_ID"),
        std::env::var("TIDAL_CLIENT_SECRET"),
    ) {
        (Ok(client_id), Ok(client_secret)) => {
            Some(TidalAuth::ClientCredentials(TidalCredentials {
                client_id,
                client_secret,
            }))
        }
        _ => None,
    }
}

fn share(options: &[String]) -> ExitCode {
    let mut now = NowPlaying::default();
    let mut to = Some(ShareTarget::Spotify);
    let mut country = "US".to_string();
    let mut explain = false;
    let mut options = options.iter();
    while let Some(option) = options.next() {
        if option == "--explain" {
            explain = true;
            continue;
        }
        match (option.as_str(), options.next()) {
            ("--title", Some(v)) => now.title = v.clone(),
            ("--artist", Some(v)) => now.artists.push(v.clone()),
            ("--duration", Some(v)) => now.duration_ms = parse_duration(v),
            ("--to", Some(v)) => to = ShareTarget::parse(v),
            ("--country", Some(v)) => country = v.clone(),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    if now.title.is_empty() || now.artists.is_empty() {
        eprintln!("--title and --artist are required\n{USAGE}");
        return ExitCode::from(2);
    }
    let Some(to) = to else {
        eprintln!("--to must be spotify, tidal or apple-music\n{USAGE}");
        return ExitCode::from(2);
    };
    let resolver = Resolver::new(ResolverConfig {
        tidal: tidal_from_env(),
        explain,
        ..Default::default()
    });
    let share = resolver.share_link(&now, to, &country);
    println!(
        "{}",
        serde_json::to_string_pretty(&share).expect("serializable")
    );
    ExitCode::SUCCESS
}

/// `4:10` or `1:02:03` → milliseconds.
fn parse_duration(value: &str) -> Option<u64> {
    value
        .split(':')
        .try_fold(0u64, |total, part| {
            Some(total * 60 + part.parse::<u64>().ok()?)
        })
        .map(|seconds| seconds * 1000)
}
