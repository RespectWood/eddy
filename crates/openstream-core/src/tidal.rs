//! Tidal lookup through the official TIDAL API (developer.tidal.com).
//!
//! Catalog reads use a client-credentials token, so a free developer app's
//! client ID and secret are enough. Items are matched exactly by ISRC (tracks)
//! or UPC/barcode (albums).
//!
//! One ISRC usually maps to many Tidal tracks: the original album plus every
//! compilation the recording was licensed to. All of them are fetched and ranked
//! (album artist, popularity, release date) so the original release opens.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ResolveError;
use crate::http::{json, map_err, Http};
use crate::matching::{best_release, is_match, release_score, search_query, Candidate, Release};
use crate::spotify::SpotifyMeta;
use crate::tidal_auth::{self, TidalAuth, TidalTokens, TokenStore};

const TOKEN_URL: &str = "https://auth.tidal.com/v1/oauth2/token";
const API: &str = "https://openapi.tidal.com/v2";
/// Pages of ISRC results to rank (20 tracks each).
const MAX_PAGES: usize = 3;
/// The API accepts up to 20 IDs per `filter[id]` request.
const IDS_PER_REQUEST: usize = 20;
/// Small preference for the exact recording found on Deezer. Small, because Deezer
/// sometimes only knows a reissue's ISRC (that only exists on compilations).
const ISRC_MATCH_BONUS: f64 = 10.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TidalCredentials {
    pub client_id: String,
    pub client_secret: String,
}

/// The chosen Tidal item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TidalItem {
    pub id: String,
    pub artist_id: Option<String>,
    pub album_title: Option<String>,
    pub album_artist: Option<String>,
}

pub(crate) struct Tidal {
    auth: TidalAuth,
    /// Client-credentials token cache. User tokens are cached in `user_tokens`.
    token: Mutex<Option<(String, Instant)>>,
    user_tokens: Mutex<Option<TidalTokens>>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default = "default_expiry")]
    expires_in: u64,
}

fn default_expiry() -> u64 {
    3600
}

/// A JSON:API document.
#[derive(Debug, Default, Deserialize)]
struct Document {
    #[serde(default)]
    data: Vec<Resource>,
    #[serde(default)]
    included: Vec<Resource>,
    #[serde(default)]
    links: Links,
}

#[derive(Debug, Default, Deserialize)]
struct Links {
    next: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Resource {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attributes: Attributes,
    #[serde(default)]
    relationships: HashMap<String, Relationship>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attributes {
    title: Option<String>,
    name: Option<String>,
    popularity: Option<f64>,
    release_date: Option<String>,
    explicit: Option<bool>,
    isrc: Option<String>,
    /// ISO 8601, e.g. `PT3M34S`.
    duration: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Relationship {
    /// An array for to-many relationships; anything else is ignored.
    #[serde(default)]
    data: Value,
}

impl Resource {
    fn related_ids(&self, relationship: &str) -> Vec<String> {
        self.relationships
            .get(relationship)
            .and_then(|r| r.data.as_array())
            .map(|refs| {
                refs.iter()
                    .filter_map(|r| r.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn release_year(&self) -> Option<u32> {
        self.attributes
            .release_date
            .as_deref()?
            .get(..4)?
            .parse()
            .ok()
    }
}

/// Included resources by (type, id).
#[derive(Debug, Default)]
struct Included(HashMap<(String, String), Resource>);

impl Included {
    fn add(&mut self, resources: Vec<Resource>) {
        for r in resources {
            self.0.insert((r.kind.clone(), r.id.clone()), r);
        }
    }

    fn get(&self, kind: &str, id: &str) -> Option<&Resource> {
        self.0.get(&(kind.to_string(), id.to_string()))
    }

    fn artist_names(&self, ids: &[String]) -> Vec<String> {
        ids.iter()
            .filter_map(|id| self.get("artists", id)?.attributes.name.clone())
            .collect()
    }
}

/// A track candidate, joined with its album and the album's artists.
#[derive(Debug, Clone)]
struct TrackCandidate {
    track: Resource,
    album: Option<Resource>,
    album_artists: Vec<String>,
    artist_ids: Vec<String>,
    /// Same ISRC as the recording matched on Deezer.
    isrc_match: bool,
}

impl TrackCandidate {
    fn score(&self, meta: &SpotifyMeta, album_hint: &str) -> f64 {
        let bonus = if self.isrc_match {
            ISRC_MATCH_BONUS
        } else {
            0.0
        };
        release_score(meta, album_hint, &self.release()) + bonus
    }

    fn release(&self) -> Release<'_> {
        Release {
            album_title: self
                .album
                .as_ref()
                .and_then(|a| a.attributes.title.as_deref())
                .unwrap_or_default(),
            album_artists: &self.album_artists,
            popularity: self.track.attributes.popularity.unwrap_or_default(),
            release_year: self.album.as_ref().and_then(Resource::release_year),
            explicit: self.track.attributes.explicit,
        }
    }
}

/// An album candidate (UPC lookups), with its artists.
#[derive(Debug, Clone)]
struct AlbumCandidate {
    album: Resource,
    artist_ids: Vec<String>,
    artist_names: Vec<String>,
}

impl AlbumCandidate {
    fn release(&self) -> Release<'_> {
        Release {
            album_title: self.album.attributes.title.as_deref().unwrap_or_default(),
            album_artists: &self.artist_names,
            popularity: self.album.attributes.popularity.unwrap_or_default(),
            release_year: self.album.release_year(),
            explicit: self.album.attributes.explicit,
        }
    }
}

impl Tidal {
    pub fn new(auth: TidalAuth) -> Self {
        Self {
            auth,
            token: Mutex::new(None),
            user_tokens: Mutex::new(None),
        }
    }

    /// Finds the original release's track. Candidates are every Tidal track with the
    /// Deezer ISRC (when known) plus Tidal's own search results that match the Spotify
    /// title, artist and duration.
    pub fn find_track(
        &self,
        http: &Http,
        meta: &SpotifyMeta,
        isrc: Option<&str>,
        album_hint: &str,
        country: &str,
        explain: bool,
    ) -> Result<Option<TidalItem>, ResolveError> {
        let mut tracks = Vec::new();
        let mut included = Included::default();
        if let Some(isrc) = isrc {
            self.tracks_by_isrc(http, isrc, country, &mut tracks, &mut included)?;
        }
        self.search_tracks(http, meta, country, &mut tracks, &mut included)?;
        let mut seen = HashSet::new();
        tracks.retain(|t| seen.insert(t.id.clone()));
        if tracks.is_empty() {
            return Ok(None);
        }

        let album_ids: Vec<String> = tracks
            .iter()
            .filter_map(|t| t.related_ids("albums").into_iter().next())
            .collect();
        let album_artists = self.album_artists(http, &album_ids, country)?;
        let candidates = join_candidates(tracks, &included, &album_artists, isrc);
        if explain {
            explain_candidates(meta, album_hint, &candidates);
        }
        Ok(pick_track(meta, album_hint, candidates).map(|c| TidalItem {
            artist_id: primary_artist_id(meta, &c.artist_ids, &included),
            album_title: c.album.as_ref().and_then(|a| a.attributes.title.clone()),
            album_artist: c.album_artists.first().cloned(),
            id: c.track.id,
        }))
    }

    /// Every track with this ISRC: the original plus its compilation appearances.
    fn tracks_by_isrc(
        &self,
        http: &Http,
        isrc: &str,
        country: &str,
        tracks: &mut Vec<Resource>,
        included: &mut Included,
    ) -> Result<(), ResolveError> {
        let mut next: Option<String> = None;
        for page in 0..MAX_PAGES {
            let request = match &next {
                // `next` is relative to the API root and carries every query parameter.
                Some(path) => http.get(&format!("{API}{path}")),
                None if page == 0 => http
                    .get(&format!("{API}/tracks"))
                    .query("countryCode", country)
                    .query("filter[isrc]", isrc)
                    // Repeated, not comma-separated: an encoded comma is rejected.
                    .query("include", "albums")
                    .query("include", "artists"),
                None => break,
            };
            let doc = self.fetch(http, request)?;
            tracks.extend(doc.data);
            included.add(doc.included);
            next = doc.links.next;
            if next.is_none() {
                break;
            }
        }
        Ok(())
    }

    /// Tidal search results that match the Spotify track (covers and remixes drop out).
    fn search_tracks(
        &self,
        http: &Http,
        meta: &SpotifyMeta,
        country: &str,
        tracks: &mut Vec<Resource>,
        included: &mut Included,
    ) -> Result<(), ResolveError> {
        let doc = self.fetch(
            http,
            http.get(&format!("{API}/searchResults"))
                .query("filter[query]", &search_query(meta))
                .query("countryCode", country)
                .query("include", "tracks")
                .query("include", "tracks.albums")
                .query("include", "tracks.artists"),
        )?;
        let (found, rest): (Vec<Resource>, Vec<Resource>) =
            doc.included.into_iter().partition(|r| r.kind == "tracks");
        included.add(rest);
        for track in found {
            let artists = included
                .artist_names(&track.related_ids("artists"))
                .join(", ");
            let candidate = Candidate {
                title: track.attributes.title.as_deref().unwrap_or_default(),
                artist: &artists,
                duration_ms: track
                    .attributes
                    .duration
                    .as_deref()
                    .and_then(iso_duration_ms),
                explicit: track.attributes.explicit,
                ..Default::default()
            };
            if is_match(meta, &candidate) {
                tracks.push(track);
            }
        }
        Ok(())
    }

    /// Finds the original edition of the album with this UPC.
    pub fn find_album(
        &self,
        http: &Http,
        meta: &SpotifyMeta,
        upc: &str,
        album_hint: &str,
        country: &str,
        explain: bool,
    ) -> Result<Option<TidalItem>, ResolveError> {
        let doc = self.fetch(
            http,
            http.get(&format!("{API}/albums"))
                .query("countryCode", country)
                .query("filter[barcodeId]", upc)
                .query("include", "artists"),
        )?;
        let mut included = Included::default();
        included.add(doc.included);
        let albums: Vec<AlbumCandidate> = doc
            .data
            .into_iter()
            .map(|album| {
                let artist_ids = album.related_ids("artists");
                AlbumCandidate {
                    artist_names: included.artist_names(&artist_ids),
                    artist_ids,
                    album,
                }
            })
            .collect();
        if explain {
            eprintln!("Tidal album candidates (album hint: {album_hint:?}):");
            for a in &albums {
                let r = a.release();
                eprintln!(
                    "  {:>6.1}  album {:<10} {} — {} (pop {:.2}, {:?})",
                    release_score(meta, album_hint, &r),
                    a.album.id,
                    r.album_title,
                    r.album_artists.join(", "),
                    r.popularity,
                    r.release_year
                );
            }
        }
        Ok(
            best_release(meta, album_hint, albums, AlbumCandidate::release).map(|a| TidalItem {
                artist_id: primary_artist_id(meta, &a.artist_ids, &included),
                album_title: a.album.attributes.title.clone(),
                album_artist: a.artist_names.first().cloned(),
                id: a.album.id,
            }),
        )
    }

    /// Album ID → album artist names, fetched in batches.
    fn album_artists(
        &self,
        http: &Http,
        album_ids: &[String],
        country: &str,
    ) -> Result<HashMap<String, Vec<String>>, ResolveError> {
        let mut unique = album_ids.to_vec();
        unique.sort();
        unique.dedup();
        let mut result = HashMap::new();
        for chunk in unique.chunks(IDS_PER_REQUEST) {
            let mut request = http
                .get(&format!("{API}/albums"))
                .query("countryCode", country)
                .query("include", "artists");
            for id in chunk {
                request = request.query("filter[id]", id);
            }
            let doc = self.fetch(http, request)?;
            let mut included = Included::default();
            included.add(doc.included);
            for album in doc.data {
                let names = included.artist_names(&album.related_ids("artists"));
                result.insert(album.id, names);
            }
        }
        Ok(result)
    }

    fn fetch(&self, http: &Http, request: ureq::Request) -> Result<Document, ResolveError> {
        let token = self.token(http)?;
        let result = json(
            request
                .set("Authorization", &format!("Bearer {token}"))
                .set("Accept", "application/vnd.api+json"),
            "Tidal",
        );
        if result.is_err() {
            // A rejected token is refreshed on the next lookup.
            *self.token.lock().unwrap_or_else(|e| e.into_inner()) = None;
            if let Some(tokens) = self
                .user_tokens
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
            {
                tokens.expires_at = 0;
            }
        }
        result
    }

    fn token(&self, http: &Http) -> Result<String, ResolveError> {
        match &self.auth {
            TidalAuth::ClientCredentials(credentials) => self.client_token(http, credentials),
            TidalAuth::User { client_id, store } => {
                self.user_token(http, client_id, store.as_ref())
            }
        }
    }

    /// The user's login, refreshed when it's about to expire. A login that can't be
    /// refreshed (revoked, expired refresh token) is cleared from the store.
    fn user_token(
        &self,
        http: &Http,
        client_id: &str,
        store: &dyn TokenStore,
    ) -> Result<String, ResolveError> {
        let mut cached = self.user_tokens.lock().unwrap_or_else(|e| e.into_inner());
        if cached.is_none() {
            *cached = store.load();
        }
        let Some(tokens) = cached.clone() else {
            return Err(ResolveError::AccountExpired);
        };
        if tokens.is_fresh() {
            return Ok(tokens.access_token);
        }
        match tidal_auth::refresh(http, client_id, &tokens)? {
            Some(fresh) => {
                store.save(fresh.clone());
                let access = fresh.access_token.clone();
                *cached = Some(fresh);
                Ok(access)
            }
            None => {
                store.clear();
                *cached = None;
                Err(ResolveError::AccountExpired)
            }
        }
    }

    fn client_token(
        &self,
        http: &Http,
        credentials: &TidalCredentials,
    ) -> Result<String, ResolveError> {
        let mut cached = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((token, expires)) = cached.as_ref() {
            if Instant::now() < *expires {
                return Ok(token.clone());
            }
        }
        let basic = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            credentials.client_id, credentials.client_secret
        ));
        let response: TokenResponse = http
            .post(TOKEN_URL)
            .set("Authorization", &format!("Basic {basic}"))
            .send_form(&[("grant_type", "client_credentials")])
            .map_err(|e| map_err("Tidal login", e))?
            .into_json()
            .map_err(|e| ResolveError::Network(format!("Tidal login: invalid response: {e}")))?;
        // Refresh a minute early so a token never expires mid-request.
        let lifetime = Duration::from_secs(response.expires_in.saturating_sub(60));
        *cached = Some((response.access_token.clone(), Instant::now() + lifetime));
        Ok(response.access_token)
    }
}

fn join_candidates(
    tracks: Vec<Resource>,
    included: &Included,
    album_artists: &HashMap<String, Vec<String>>,
    isrc: Option<&str>,
) -> Vec<TrackCandidate> {
    tracks
        .into_iter()
        .map(|track| {
            let album_id = track.related_ids("albums").into_iter().next();
            TrackCandidate {
                album: album_id
                    .as_deref()
                    .and_then(|id| included.get("albums", id))
                    .cloned(),
                album_artists: album_id
                    .and_then(|id| album_artists.get(&id).cloned())
                    .unwrap_or_default(),
                artist_ids: track.related_ids("artists"),
                isrc_match: isrc.is_some() && track.attributes.isrc.as_deref() == isrc,
                track,
            }
        })
        .collect()
}

/// The best-scoring candidate. The first one wins ties.
fn pick_track(
    meta: &SpotifyMeta,
    album_hint: &str,
    candidates: Vec<TrackCandidate>,
) -> Option<TrackCandidate> {
    let mut best: Option<(f64, TrackCandidate)> = None;
    for c in candidates {
        let s = c.score(meta, album_hint);
        if best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, c));
        }
    }
    best.map(|(_, c)| c)
}

/// Parses ISO 8601 durations such as `PT3M34S` or `PT1H2M3.5S` into milliseconds.
fn iso_duration_ms(value: &str) -> Option<u64> {
    let rest = value.strip_prefix("PT")?;
    let mut total = 0.0;
    let mut number = String::new();
    for c in rest.chars() {
        match c {
            '0'..='9' | '.' => number.push(c),
            'H' | 'M' | 'S' => {
                let n: f64 = number.parse().ok()?;
                total += n * match c {
                    'H' => 3_600_000.0,
                    'M' => 60_000.0,
                    _ => 1_000.0,
                };
                number.clear();
            }
            _ => return None,
        }
    }
    number.is_empty().then_some(total as u64)
}

/// The artist matching Spotify's primary artist, else the first one listed.
fn primary_artist_id(meta: &SpotifyMeta, ids: &[String], included: &Included) -> Option<String> {
    let wanted = crate::matching::normalize(meta.primary_artist());
    ids.iter()
        .find(|id| {
            included
                .get("artists", id)
                .and_then(|a| a.attributes.name.as_deref())
                .is_some_and(|name| crate::matching::normalize(name) == wanted)
        })
        .or_else(|| ids.first())
        .cloned()
}

fn explain_candidates(meta: &SpotifyMeta, album_hint: &str, candidates: &[TrackCandidate]) {
    eprintln!("Tidal candidates (album hint: {album_hint:?}):");
    let mut scored: Vec<(f64, &TrackCandidate)> = candidates
        .iter()
        .map(|c| (c.score(meta, album_hint), c))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (score, c) in scored {
        let r = c.release();
        eprintln!(
            "  {score:>6.1}  track {:<10} {} — {} (pop {:.2}, {:?}){}",
            c.track.id,
            r.album_title,
            r.album_artists.join(", "),
            r.popularity,
            r.release_year,
            if c.isrc_match { " [ISRC]" } else { "" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::SpotifyKind;

    fn usher() -> SpotifyMeta {
        SpotifyMeta {
            kind: SpotifyKind::Track,
            title: "Yeah! (feat. Lil Jon & Ludacris)".into(),
            artists: vec!["USHER".into(), "Lil Jon".into(), "Ludacris".into()],
            duration_ms: Some(250_373),
            explicit: Some(false),
            track_count: None,
        }
    }

    /// Recorded `GET /v2/tracks?filter[isrc]=USAR10301423&include=albums&include=artists`
    /// (first page) and the matching `/v2/albums?filter[id]=…&include=artists` response.
    #[test]
    fn picks_original_album_from_recorded_response() {
        let tracks: Document =
            serde_json::from_str(include_str!("../tests/fixtures/tidal_isrc_usher.json")).unwrap();
        let albums: Document =
            serde_json::from_str(include_str!("../tests/fixtures/tidal_albums_usher.json"))
                .unwrap();
        assert!(tracks.data.len() > 10, "fixture has the compilations");

        let mut album_artists = HashMap::new();
        let mut album_included = Included::default();
        album_included.add(albums.included);
        for album in albums.data {
            album_artists.insert(
                album.id.clone(),
                album_included.artist_names(&album.related_ids("artists")),
            );
        }
        let mut included = Included::default();
        included.add(tracks.included);
        let candidates =
            join_candidates(tracks.data, &included, &album_artists, Some("USAR10301423"));
        assert!(candidates.iter().all(|c| c.isrc_match));
        let meta = usher();
        let picked =
            pick_track(&meta, "Confessions (Expanded Edition)", candidates.clone()).unwrap();
        assert_eq!(picked.track.id, "11337915");

        // Also without the Deezer album hint.
        let picked = pick_track(&meta, "", candidates).unwrap();
        assert_eq!(picked.track.id, "11337915");
        let artist_id = primary_artist_id(&meta, &picked.artist_ids, &included).unwrap();
        let artist = included.get("artists", &artist_id).unwrap();
        assert_eq!(
            artist
                .attributes
                .name
                .as_deref()
                .map(str::to_lowercase)
                .as_deref(),
            Some("usher")
        );
    }

    #[test]
    fn parses_iso_durations() {
        assert_eq!(iso_duration_ms("PT3M34S"), Some(214_000));
        assert_eq!(iso_duration_ms("PT1H2M3.5S"), Some(3_723_500));
        assert_eq!(iso_duration_ms("PT45S"), Some(45_000));
        assert_eq!(iso_duration_ms("3M"), None);
        assert_eq!(iso_duration_ms("PT3"), None);
    }

    /// Needs `TIDAL_CLIENT_ID` / `TIDAL_CLIENT_SECRET`: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_usher_opens_confessions() {
        let (Ok(client_id), Ok(client_secret)) = (
            std::env::var("TIDAL_CLIENT_ID"),
            std::env::var("TIDAL_CLIENT_SECRET"),
        ) else {
            eprintln!("skipped: TIDAL_CLIENT_ID / TIDAL_CLIENT_SECRET not set");
            return;
        };
        let tidal = Tidal::new(TidalAuth::ClientCredentials(TidalCredentials {
            client_id,
            client_secret,
        }));
        let item = tidal
            .find_track(
                &Http::new(),
                &usher(),
                Some("USAR10301423"),
                "Confessions (Expanded Edition)",
                "SE",
                false,
            )
            .unwrap()
            .unwrap();
        assert_eq!(item.id, "11337915");
        assert_eq!(item.album_artist.as_deref(), Some("Usher"));
    }
}
