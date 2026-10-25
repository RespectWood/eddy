//! Every network request goes through [`Http::send`]. Call sites build a [`Request`];
//! the live backend sends it with `ureq`, and tests answer it from recorded responses
//! (see `replay.rs`), so whole lookups run offline.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::ResolveError;

const USER_AGENT: &str = concat!("eddy/", env!("CARGO_PKG_VERSION"));

/// Spotify only serves the embed page data to browser-like clients.
pub(crate) const BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub method: Method,
    pub url: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(&'static str, String)>,
    /// `application/x-www-form-urlencoded` body (POST).
    pub form: Vec<(String, String)>,
    /// `false`: a 3xx response is returned as is, so the caller can check each hop.
    pub follow_redirects: bool,
}

impl Request {
    pub fn get(url: &str) -> Self {
        Self {
            method: Method::Get,
            url: url.to_string(),
            query: Vec::new(),
            headers: Vec::new(),
            form: Vec::new(),
            follow_redirects: true,
        }
    }

    pub fn post_form(url: &str, form: &[(&str, &str)]) -> Self {
        Self {
            method: Method::Post,
            form: form
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Self::get(url)
        }
    }

    /// Appends a query parameter; repeated names are sent repeatedly.
    pub fn query(mut self, name: &str, value: &str) -> Self {
        self.query.push((name.to_string(), value.to_string()));
        self
    }

    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_string()));
        self
    }

    pub fn without_redirects(mut self) -> Self {
        self.follow_redirects = false;
        self
    }

    /// `METHOD url` with every query parameter sorted, so recordings match requests
    /// regardless of parameter order. Headers and form bodies aren't part of it: they
    /// carry secrets and are never recorded.
    #[cfg(any(test, feature = "test-support"))]
    pub fn key(&self) -> String {
        format!(
            "{} {}",
            self.method.as_str(),
            canonical_url(&self.url, &self.query)
        )
    }
}

/// `url` plus `extra` query parameters, all sorted.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn canonical_url(url: &str, extra: &[(String, String)]) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.to_string();
    };
    let mut pairs: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
    pairs.extend(extra.iter().cloned());
    pairs.sort();
    parsed.set_query(None);
    if !pairs.is_empty() {
        parsed.query_pairs_mut().extend_pairs(pairs);
    }
    parsed.to_string()
}

/// Any HTTP status; only transport failures are errors at this level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Response {
    pub status: u16,
    /// The `Location` header of a redirect.
    pub location: Option<String>,
    pub body: String,
}

#[derive(Clone)]
pub(crate) struct Http {
    backend: Backend,
}

#[derive(Clone)]
enum Backend {
    Live {
        agent: ureq::Agent,
        /// Returns redirects instead of following them.
        manual: ureq::Agent,
        #[cfg(test)]
        recorder: Option<std::sync::Arc<crate::replay::Recorder>>,
    },
    #[cfg(any(test, feature = "test-support"))]
    Replay(std::sync::Arc<crate::replay::Replay>),
}

impl Http {
    pub fn new() -> Self {
        let builder = || {
            ureq::AgentBuilder::new()
                // Every API is HTTPS; this also refuses redirects to plain HTTP.
                .https_only(true)
                .timeout_connect(Duration::from_secs(4))
                .timeout(Duration::from_secs(10))
                .user_agent(USER_AGENT)
        };
        Self {
            backend: Backend::Live {
                agent: builder().redirects(5).build(),
                manual: builder().redirects(0).build(),
                #[cfg(test)]
                recorder: None,
            },
        }
    }

    /// Answers every request from `replay` instead of the network.
    #[cfg(any(test, feature = "test-support"))]
    pub fn replay(replay: std::sync::Arc<crate::replay::Replay>) -> Self {
        Self {
            backend: Backend::Replay(replay),
        }
    }

    /// Live, and saves every exchange to the cassette `name` (see `replay.rs`).
    #[cfg(test)]
    pub fn recording(name: &str) -> Self {
        let mut http = Self::new();
        if let Backend::Live { recorder, .. } = &mut http.backend {
            *recorder = Some(std::sync::Arc::new(crate::replay::Recorder::new(name)));
        }
        http
    }

    /// Whether requests reach real servers, whose rate limits apply.
    pub fn is_live(&self) -> bool {
        matches!(self.backend, Backend::Live { .. })
    }

    /// Sends `request`. Any HTTP status is a response; only transport failures (DNS,
    /// timeout, TLS) are errors. `service` names the API in error messages.
    pub fn send(&self, request: &Request, service: &'static str) -> Result<Response, ResolveError> {
        match &self.backend {
            Backend::Live {
                agent,
                manual,
                #[cfg(test)]
                recorder,
            } => {
                let agent = if request.follow_redirects {
                    agent
                } else {
                    manual
                };
                let result = send_live(agent, request);
                #[cfg(test)]
                if let (Some(recorder), Ok(response)) = (recorder, &result) {
                    recorder.record(request, response);
                }
                result.map_err(|e| ResolveError::Network(format!("{service}: {e}")))
            }
            #[cfg(any(test, feature = "test-support"))]
            Backend::Replay(replay) => replay
                .answer(request)
                .map_err(|e| ResolveError::Network(format!("{service}: {e}"))),
        }
    }

    /// `send`, with error statuses turned into errors.
    pub fn fetch(
        &self,
        request: &Request,
        service: &'static str,
    ) -> Result<Response, ResolveError> {
        let response = self.send(request, service)?;
        match response.status {
            400.. => Err(status_error(service, response.status)),
            _ => Ok(response),
        }
    }

    /// `fetch` and decode the JSON body.
    pub fn json<T: DeserializeOwned>(
        &self,
        request: &Request,
        service: &'static str,
    ) -> Result<T, ResolveError> {
        let response = self.fetch(request, service)?;
        serde_json::from_str(&response.body)
            .map_err(|e| ResolveError::Network(format!("{service}: invalid response: {e}")))
    }
}

fn send_live(agent: &ureq::Agent, request: &Request) -> Result<Response, String> {
    let mut call = agent.request(request.method.as_str(), &request.url);
    for (name, value) in &request.query {
        call = call.query(name, value);
    }
    for (name, value) in &request.headers {
        call = call.set(name, value);
    }
    let result = match request.method {
        Method::Get => call.call(),
        Method::Post => {
            let form: Vec<(&str, &str)> = request
                .form
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            call.send_form(&form)
        }
    };
    let response = match result {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response,
        Err(ureq::Error::Transport(t)) => return Err(t.to_string()),
    };
    let status = response.status();
    let location = response.header("Location").map(str::to_string);
    let body = response.into_string().map_err(|e| e.to_string())?;
    Ok(Response {
        status,
        location,
        body,
    })
}

/// The error for an HTTP error status.
pub(crate) fn status_error(service: &'static str, status: u16) -> ResolveError {
    match status {
        429 => ResolveError::RateLimited(service),
        code => ResolveError::Network(format!("{service} returned HTTP {code}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_query_parameters() {
        let a = Request::get("https://openapi.tidal.com/v2/tracks?countryCode=SE")
            .query("include", "artists")
            .query("include", "albums")
            .query("filter[isrc]", "USAR10301423");
        let b = Request::get("https://openapi.tidal.com/v2/tracks")
            .query("filter[isrc]", "USAR10301423")
            .query("include", "albums")
            .query("countryCode", "SE")
            .query("include", "artists");
        assert_eq!(a.key(), b.key());
        assert_eq!(
            a.key(),
            "GET https://openapi.tidal.com/v2/tracks?countryCode=SE&filter%5Bisrc%5D=USAR10301423&include=albums&include=artists"
        );
        assert_eq!(
            Request::post_form("https://auth.tidal.com/v1/oauth2/token", &[("secret", "x")]).key(),
            "POST https://auth.tidal.com/v1/oauth2/token"
        );
    }

    #[test]
    fn maps_error_statuses() {
        assert!(matches!(
            status_error("Deezer", 429),
            ResolveError::RateLimited("Deezer")
        ));
        assert_eq!(
            status_error("Deezer", 500).to_string(),
            "Deezer returned HTTP 500"
        );
    }
}
