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
}

impl Http {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(4))
            .timeout(Duration::from_secs(10))
            .redirects(5)
            .user_agent(USER_AGENT)
            .build();
        Self { agent }
    }

    pub fn get(&self, url: &str) -> ureq::Request {
        self.agent.get(url)
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
