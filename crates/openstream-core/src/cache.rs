//! Local SQLite cache of resolved conversions.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::link::SpotifyLink;
use crate::resolve::Conversion;
use crate::target::Service;

/// Catalogs change (regional availability, re-releases), so entries expire.
const MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

pub(crate) struct Cache {
    conn: Connection,
}

impl Cache {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversions (
                spotify_kind TEXT NOT NULL,
                spotify_id   TEXT NOT NULL,
                service      TEXT NOT NULL,
                country      TEXT NOT NULL,
                conversion   TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                PRIMARY KEY (spotify_kind, spotify_id, service, country)
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn get(&self, link: &SpotifyLink, service: Service, country: &str) -> Option<Conversion> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT conversion FROM conversions
                 WHERE spotify_kind = ?1 AND spotify_id = ?2 AND service = ?3 AND country = ?4
                   AND created_at > ?5",
                params![
                    link.kind.as_str(),
                    link.id,
                    service.as_str(),
                    country,
                    now() - MAX_AGE_SECS
                ],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten();
        json.and_then(|j| serde_json::from_str(&j).ok())
    }

    pub fn put(
        &self,
        link: &SpotifyLink,
        service: Service,
        country: &str,
        conversion: &Conversion,
    ) {
        let Ok(json) = serde_json::to_string(conversion) else {
            return;
        };
        // A failed write only costs a repeat lookup later, so errors are ignored.
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO conversions VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                link.kind.as_str(),
                link.id,
                service.as_str(),
                country,
                json,
                now()
            ],
        );
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::SpotifyKind;

    #[test]
    fn round_trips_conversions() {
        let cache = Cache::in_memory().unwrap();
        let link = SpotifyLink {
            kind: SpotifyKind::Track,
            id: "4uLU6hMCjMI75M1A2tKUQC".into(),
        };
        let conversion = Conversion {
            service: Service::Tidal,
            spotify_url: link.canonical_url(),
            title: "Never Gonna Give You Up".into(),
            artist: "Rick Astley".into(),
            native_url: "tidal://track/1".into(),
            web_url: "https://tidal.com/browse/track/1".into(),
            from_cache: false,
        };
        assert!(cache.get(&link, Service::Tidal, "DE").is_none());
        cache.put(&link, Service::Tidal, "DE", &conversion);
        assert_eq!(cache.get(&link, Service::Tidal, "DE"), Some(conversion));
        assert!(cache.get(&link, Service::Tidal, "US").is_none());
        assert!(cache.get(&link, Service::AppleMusic, "DE").is_none());
    }
}
