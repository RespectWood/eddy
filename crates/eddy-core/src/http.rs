use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::error::ResolveError;

const USER_AGENT: &str = concat!("eddy/", env!("CARGO_PKG_VERSION"));

/// Spotify only serves the embed page data to browser-like clients.
pub(crate) const BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

#[derive(Clone)]
pub(crate) struct Http {
    agent: ureq::Agent,
    /// Returns redirects instead of following them, so the caller can check each hop.
    manual: ureq::Agent,
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
            agent: builder().redirects(5).build(),
            manual: builder().redirects(0).build(),
        }
    }

    pub fn get(&self, url: &str) -> ureq::Request {
        self.agent.get(url)
    }

    /// A GET whose 3xx response is returned as is.
    pub fn get_without_redirects(&self, url: &str) -> ureq::Request {
        self.manual.get(url)
    }

    pub fn post(&self, url: &str) -> ureq::Request {
        self.agent.post(url)
    }
}

/// Sends a request and decodes the JSON body. `service` names the API in error messages.
pub(crate) fn json<T: DeserializeOwned>(
    request: ureq::Request,
    service: &'static str,
) -> Result<T, ResolveError> {
    let response = request.call().map_err(|e| map_err(service, e))?;
    response
        .into_json()
        .map_err(|e| ResolveError::Network(format!("{service}: invalid response: {e}")))
}

pub(crate) fn map_err(service: &'static str, error: ureq::Error) -> ResolveError {
    match error {
        ureq::Error::Status(429, _) => ResolveError::RateLimited(service),
        ureq::Error::Status(code, _) => {
            ResolveError::Network(format!("{service} returned HTTP {code}"))
        }
        ureq::Error::Transport(t) => ResolveError::Network(format!("{service}: {t}")),
    }
}
