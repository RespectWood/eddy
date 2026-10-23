//! "Connect Tidal": OAuth 2.1 Authorization Code flow with PKCE.
//!
//! The user logs in on Tidal's own login page; Eddy only ever holds a public
//! client ID (no secret). The app opens [`LoginRequest::authorize_url`] in a browser
//! sheet, Tidal redirects to the redirect URI with a code, and [`finish_login`] trades
//! that code for tokens. Tokens are stored by the app (Keychain) through [`TokenStore`].

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::ResolveError;
use crate::http::Http;

const AUTHORIZE_URL: &str = "https://login.tidal.com/authorize";
const TOKEN_URL: &str = "https://auth.tidal.com/v1/oauth2/token";
const PROFILE_URL: &str = "https://openapi.tidal.com/v2/users/me";

/// Catalog reads need no scope, and new developer apps have no scopes allowed, so none
/// are requested. (`user.read` would add the account's name and country; the profile
/// lookup is optional and simply skipped when it isn't allowed.)
pub const DEFAULT_SCOPES: &str = "";

/// A login in progress. Kept by the app until the redirect arrives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginRequest {
    pub authorize_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub state: String,
    /// PKCE secret for this one login; never leaves the Mac.
    pub code_verifier: String,
}

/// The user's Tidal login.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TidalTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    /// Display name for "Connected as …", from the profile.
    pub user_name: Option<String>,
    /// Account country, used as the lookup country.
    pub country: Option<String>,
}

impl TidalTokens {
    /// Valid for at least another minute.
    pub fn is_fresh(&self) -> bool {
        self.expires_at > now() + 60
    }
}

/// Where tokens live. Implemented by the app with the Keychain; the core never stores them.
pub trait TokenStore: Send + Sync {
    fn load(&self) -> Option<TidalTokens>;
    fn save(&self, tokens: TidalTokens);
    /// The login is no longer valid (revoked or expired refresh token).
    fn clear(&self);
}

/// How the core authenticates against the TIDAL API.
#[derive(Clone)]
pub enum TidalAuth {
    /// Developer mode: an app's client ID and secret (client-credentials grant).
    ClientCredentials(crate::TidalCredentials),
    /// "Connect Tidal": the user's own login.
    User {
        client_id: String,
        store: Arc<dyn TokenStore>,
    },
}

/// Never prints secrets or tokens.
impl std::fmt::Debug for TidalAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClientCredentials(_) => f.write_str("TidalAuth::ClientCredentials(..)"),
            Self::User { .. } => f.write_str("TidalAuth::User(..)"),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("Tidal login was cancelled or denied: {0}")]
    Denied(String),
    #[error("the login response didn't match this login (state mismatch)")]
    StateMismatch,
    #[error("the login response had no code")]
    MissingCode,
    #[error("{0}")]
    Network(String),
}

/// Starts a login: fresh PKCE verifier, challenge and state.
pub fn login_request(client_id: &str, redirect_uri: &str, scopes: &str) -> LoginRequest {
    let code_verifier = random_token(48);
    let state = random_token(16);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
    let mut url = Url::parse(AUTHORIZE_URL).expect("valid constant");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge)
        .append_pair("state", &state);
    if !scopes.is_empty() {
        url.query_pairs_mut().append_pair("scope", scopes);
    }
    LoginRequest {
        authorize_url: url.to_string(),
        client_id: client_id.to_string(),
        redirect_uri: redirect_uri.to_string(),
        state,
        code_verifier,
    }
}

/// Reads the code from the redirect Tidal sent back. Pure, so it's unit-tested.
pub fn code_from_callback(request: &LoginRequest, callback_url: &str) -> Result<String, AuthError> {
    let url = Url::parse(callback_url).map_err(|_| AuthError::MissingCode)?;
    let param = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(error) = param("error") {
        let description = param("error_description").unwrap_or_default();
        return Err(AuthError::Denied(
            format!("{error} {description}").trim().to_string(),
        ));
    }
    if param("state").as_deref() != Some(request.state.as_str()) {
        return Err(AuthError::StateMismatch);
    }
    param("code").ok_or(AuthError::MissingCode)
}

/// Completes a login: exchanges the code for tokens and reads the profile.
pub fn finish_login(request: &LoginRequest, callback_url: &str) -> Result<TidalTokens, AuthError> {
    let code = code_from_callback(request, callback_url)?;
    let http = Http::new();
    let response = token_request(
        &http,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("client_id", &request.client_id),
            ("redirect_uri", &request.redirect_uri),
            ("code_verifier", &request.code_verifier),
        ],
    )?;
    let mut tokens = response.into_tokens(None);
    // The profile is a nicety ("Connected as …", country); a failure doesn't fail the login.
    if let Ok((name, country)) = profile(&http, &tokens.access_token) {
        tokens.user_name = name;
        tokens.country = country;
    }
    Ok(tokens)
}

/// Gets a new access token. `Ok(None)` means the login is no longer valid.
pub(crate) fn refresh(
    http: &Http,
    client_id: &str,
    tokens: &TidalTokens,
) -> Result<Option<TidalTokens>, ResolveError> {
    let Some(refresh_token) = &tokens.refresh_token else {
        return Ok(None);
    };
    match token_request(
        http,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ],
    ) {
        Ok(response) => {
            let mut fresh = response.into_tokens(Some(refresh_token.clone()));
            fresh.user_name = tokens.user_name.clone();
            fresh.country = tokens.country.clone();
            Ok(Some(fresh))
        }
        // 4xx: revoked or expired refresh token (`invalid_grant`, `invalid_client`, …).
        Err(AuthError::Denied(_)) => Ok(None),
        Err(e) => Err(ResolveError::Network(e.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(default = "default_expiry")]
    expires_in: u64,
}

fn default_expiry() -> u64 {
    3600
}

impl TokenResponse {
    /// Some refresh responses omit the refresh token; then the old one stays valid.
    fn into_tokens(self, previous_refresh: Option<String>) -> TidalTokens {
        TidalTokens {
            access_token: self.access_token,
            refresh_token: self.refresh_token.or(previous_refresh),
            expires_at: now() + self.expires_in,
            user_name: None,
            country: None,
        }
    }
}

fn token_request(http: &Http, form: &[(&str, &str)]) -> Result<TokenResponse, AuthError> {
    match http.post(TOKEN_URL).send_form(form) {
        Ok(response) => response
            .into_json()
            .map_err(|e| AuthError::Network(format!("Tidal login: invalid response: {e}"))),
        Err(ureq::Error::Status(code, response)) if (400..500).contains(&code) => {
            let body = response.into_string().unwrap_or_default();
            Err(AuthError::Denied(format!(
                "HTTP {code}: {}",
                body.chars().take(200).collect::<String>()
            )))
        }
        Err(e) => Err(AuthError::Network(format!("Tidal login: {e}"))),
    }
}

/// (display name, country) from `/v2/users/me`.
fn profile(http: &Http, access_token: &str) -> Result<(Option<String>, Option<String>), AuthError> {
    let doc: serde_json::Value = http
        .get(PROFILE_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("Accept", "application/vnd.api+json")
        .call()
        .map_err(|e| AuthError::Network(e.to_string()))?
        .into_json()
        .map_err(|e| AuthError::Network(e.to_string()))?;
    let attributes = &doc["data"]["attributes"];
    let text = |key: &str| {
        attributes[key]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let name = text("username")
        .or_else(|| text("email"))
        .or_else(|| text("firstName"));
    Ok((name, text("country")))
}

fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    getrandom::getrandom(&mut buffer).expect("system random source");
    URL_SAFE_NO_PAD.encode(buffer)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_pkce_authorize_url() {
        let request = login_request("client123", "eddy://auth/tidal", DEFAULT_SCOPES);
        let url = Url::parse(&request.authorize_url).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("login.tidal.com"));
        assert_eq!(params["response_type"], "code");
        assert_eq!(params["client_id"], "client123");
        assert_eq!(params["redirect_uri"], "eddy://auth/tidal");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["state"], request.state);
        // The challenge is the SHA-256 of the verifier, base64url without padding.
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(request.code_verifier.as_bytes()));
        assert_eq!(params["code_challenge"], expected);
        // RFC 7636: 43–128 characters.
        assert!((43..=128).contains(&request.code_verifier.len()));
        assert!(!params.contains_key("client_secret"));
        assert!(
            !params.contains_key("scope"),
            "no scopes requested by default"
        );
        let scoped = login_request("c", "eddy://auth/tidal", "user.read");
        assert!(scoped.authorize_url.contains("scope=user.read"));
    }

    #[test]
    fn every_login_gets_fresh_secrets() {
        let a = login_request("c", "eddy://auth/tidal", DEFAULT_SCOPES);
        let b = login_request("c", "eddy://auth/tidal", DEFAULT_SCOPES);
        assert_ne!(a.state, b.state);
        assert_ne!(a.code_verifier, b.code_verifier);
    }

    #[test]
    fn reads_code_from_callback() {
        let request = login_request("c", "eddy://auth/tidal", DEFAULT_SCOPES);
        let ok = format!("eddy://auth/tidal?code=abc&state={}", request.state);
        assert_eq!(code_from_callback(&request, &ok), Ok("abc".to_string()));

        let forged = "eddy://auth/tidal?code=abc&state=someone-else";
        assert_eq!(
            code_from_callback(&request, forged),
            Err(AuthError::StateMismatch)
        );

        let denied = format!(
            "eddy://auth/tidal?error=access_denied&error_description=User+cancelled&state={}",
            request.state
        );
        assert_eq!(
            code_from_callback(&request, &denied),
            Err(AuthError::Denied(
                "access_denied User cancelled".to_string()
            ))
        );

        let no_code = format!("eddy://auth/tidal?state={}", request.state);
        assert_eq!(
            code_from_callback(&request, &no_code),
            Err(AuthError::MissingCode)
        );
    }

    #[test]
    fn token_freshness() {
        let mut tokens = TidalTokens {
            access_token: "a".into(),
            refresh_token: None,
            expires_at: now() + 3600,
            user_name: None,
            country: None,
        };
        assert!(tokens.is_fresh());
        tokens.expires_at = now() + 30;
        assert!(!tokens.is_fresh());
    }

    /// The token endpoint answers `invalid_grant` (not `invalid_client`) to a bogus code
    /// without a secret, i.e. it accepts public clients. `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn live_token_endpoint_accepts_public_clients() {
        let request = login_request("public-client-check", "eddy://auth/tidal", DEFAULT_SCOPES);
        let callback = format!("eddy://auth/tidal?code=bogus&state={}", request.state);
        match finish_login(&request, &callback) {
            Err(AuthError::Denied(message)) => assert!(message.contains("HTTP 400"), "{message}"),
            other => panic!("{other:?}"),
        }
    }
}
