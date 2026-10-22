//! Local SQLite cache of resolved conversions.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::link::SpotifyLink;
use crate::resolve::Conversion;
use crate::target::Service;

/// Catalogs change (regional availability, re-releases), so entries expire.
const MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// Bump when matching changes enough that earlier results shouldn't be reused.
/// v2: Tidal/Apple Music prefer the original release over compilations (ticket 0002).
const TABLE: &str = "conversions_v2";
const OLD_TABLES: &[&str] = &["conversions"];

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
        for old in OLD_TABLES {
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {old};"))?;
        }
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {TABLE} (
                spotify_kind TEXT NOT NULL,
                spotify_id   TEXT NOT NULL,
                service      TEXT NOT NULL,
                country      TEXT NOT NULL,
                conversion   TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                PRIMARY KEY (spotify_kind, spotify_id, service, country)
            );"
        ))?;
        Ok(Self { conn })
    }

    pub fn get(&self, link: &SpotifyLink, service: Service, country: &str) -> Option<Conversion> {
        let json: Option<String> = self
            .conn
            .query_row(
                &format!(
                    "SELECT conversion FROM {TABLE}
                     WHERE spotify_kind = ?1 AND spotify_id = ?2 AND service = ?3 AND country = ?4
                       AND created_at > ?5"
                ),
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
            &format!("INSERT OR REPLACE INTO {TABLE} VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
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
            kind: SpotifyKind::Track,
            spotify_url: link.canonical_url(),
            title: "Never Gonna Give You Up".into(),
            artist: "Rick Astley".into(),
            album: Some("Whenever You Need Somebody".into()),
            album_artist: Some("Rick Astley".into()),
            native_url: "tidal://track/1".into(),
            web_url: "https://tidal.com/browse/track/1".into(),
            artist_native_url: Some("tidal://artist/2".into()),
            artist_web_url: Some("https://tidal.com/browse/artist/2".into()),
            from_cache: false,
        };
        assert!(cache.get(&link, Service::Tidal, "DE").is_none());
        cache.put(&link, Service::Tidal, "DE", &conversion);
        assert_eq!(cache.get(&link, Service::Tidal, "DE"), Some(conversion));
        assert!(cache.get(&link, Service::Tidal, "US").is_none());
        assert!(cache.get(&link, Service::AppleMusic, "DE").is_none());
    }
}
