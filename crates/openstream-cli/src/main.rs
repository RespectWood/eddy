//! `openstream` command-line tool, for development and testing.
//!
//! ```text
//! openstream parse <url>
//! openstream resolve <url> --to tidal|apple-music [--country DE] [--cache <path>] [--explain]
//! ```
//!
//! `--explain` prints every candidate and its score to stderr.
//!
//! Tidal credentials are read from `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET`.

use std::path::PathBuf;
use std::process::ExitCode;

use openstream_core::{
    parse, ResolveError, Resolver, ResolverConfig, Service, TidalAuth, TidalCredentials,
};
use serde_json::json;

const USAGE: &str = "usage:
  openstream parse <url>
  openstream resolve <url> --to tidal|apple-music [--country DE] [--cache <path>] [--explain]";

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
    let tidal = match (
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
    };
    let resolver = Resolver::new(ResolverConfig {
        cache_path,
        tidal,
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
