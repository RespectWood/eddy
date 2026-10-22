//! Tidal lookup through the official TIDAL API (developer.tidal.com).
//!
//! Catalog reads use a client-credentials token, so a free developer app's
//! client ID and secret are enough. Items are matched exactly by ISRC (tracks)
//! or UPC/barcode (albums).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;

use crate::error::ResolveError;
use crate::http::{json, map_err, Http};
use crate::target::ItemKind;

const TOKEN_URL: &str = "https://auth.tidal.com/v1/oauth2/token";
const API: &str = "https://openapi.tidal.com/v2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TidalCredentials {
    pub client_id: String,
    pub client_secret: String,
}

pub(crate) struct Tidal {
    credentials: TidalCredentials,
    token: Mutex<Option<(String, Instant)>>,
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

#[derive(Debug, Deserialize)]
struct Document {
    #[serde(default)]
    data: Vec<Resource>,
}

#[derive(Debug, Deserialize)]
struct Resource {
    id: String,
}

impl Tidal {
    pub fn new(credentials: TidalCredentials) -> Self {
        Self {
            credentials,
            token: Mutex::new(None),
        }
    }

    /// Finds the Tidal ID of the track with this ISRC, or the album with this UPC.
    pub fn find(
        &self,
        http: &Http,
        kind: ItemKind,
        code: &str,
        country: &str,
    ) -> Result<Option<String>, ResolveError> {
        let (path, filter) = match kind {
            ItemKind::Track => ("tracks", "filter[isrc]"),
            ItemKind::Album => ("albums", "filter[barcodeId]"),
        };
        let token = self.token(http)?;
        let result: Result<Document, _> = json(
            http.get(&format!("{API}/{path}"))
                .set("Authorization", &format!("Bearer {token}"))
                .set("Accept", "application/vnd.api+json")
                .query("countryCode", country)
                .query(filter, code),
            "Tidal",
        );
        if result.is_err() {
            // A rejected token is refreshed on the next lookup.
            *self.token.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        Ok(result?.data.into_iter().next().map(|r| r.id))
    }

    fn token(&self, http: &Http) -> Result<String, ResolveError> {
        let mut cached = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((token, expires)) = cached.as_ref() {
            if Instant::now() < *expires {
                return Ok(token.clone());
            }
        }
        let basic = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            self.credentials.client_id, self.credentials.client_secret
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
