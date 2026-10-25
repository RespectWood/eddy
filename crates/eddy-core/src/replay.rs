//! Recorded responses for offline tests.
//!
//! A cassette is a JSON file in `tests/fixtures/cassettes/` with one entry per
//! request/response. Live tests write them when run with `EDDY_RECORD=1`; offline
//! tests replay them through [`Http::replay`](crate::http::Http::replay). Tests can
//! also add hand-written entries (errors, redirects) with [`Replay::with`].
//!
//! Only the request line (method, URL, query) and the response are saved: request
//! headers and form bodies carry secrets and are never written, and tokens in
//! responses are replaced by `REDACTED`.

// Builds with only the `test-support` feature (the CLI's tests) use just the replay
// part; the rest is for this crate's own tests.
#![cfg_attr(not(test), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::{canonical_url, Request, Response};

/// Response fields that hold credentials. Their values are never saved.
pub(crate) const SECRET_KEYS: &[&str] = &[
    "access_token",
    "refresh_token",
    "id_token",
    "accessToken",
    "clientToken",
];

pub(crate) const REDACTED: &str = "REDACTED";

pub(crate) fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn cassette_path(name: &str) -> PathBuf {
    fixtures_dir()
        .join("cassettes")
        .join(format!("{name}.json"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Cassette {
    /// Where the responses came from and when.
    #[serde(default)]
    pub note: String,
    pub exchanges: Vec<Exchange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exchange {
    /// `METHOD url`. A URL without a query matches that path with any query.
    pub request: String,
    #[serde(default = "ok")]
    pub status: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// A JSON body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<Value>,
    /// Any other body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The body is another fixture file, relative to `tests/fixtures/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// No response at all: a transport failure (timeout, DNS) with this message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_error: Option<String>,
}

fn ok() -> u16 {
    200
}

impl Exchange {
    fn new(request: &str) -> Self {
        Self {
            request: request.to_string(),
            status: 200,
            location: None,
            json: None,
            text: None,
            file: None,
            transport_error: None,
        }
    }

    pub fn json(request: &str, body: Value) -> Self {
        Self {
            json: Some(body),
            ..Self::new(request)
        }
    }

    pub fn text(request: &str, body: &str) -> Self {
        Self {
            text: Some(body.to_string()),
            ..Self::new(request)
        }
    }

    pub fn status(request: &str, status: u16) -> Self {
        Self {
            status,
            ..Self::new(request)
        }
    }

    pub fn redirect(request: &str, location: &str) -> Self {
        Self {
            status: 301,
            location: Some(location.to_string()),
            ..Self::new(request)
        }
    }

    pub fn transport_error(request: &str, message: &str) -> Self {
        Self {
            transport_error: Some(message.to_string()),
            ..Self::new(request)
        }
    }

    pub fn with_status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    fn body(&self) -> String {
        if let Some(json) = &self.json {
            return json.to_string();
        }
        if let Some(file) = &self.file {
            let path = fixtures_dir().join(file);
            return std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()));
        }
        self.text.clone().unwrap_or_default()
    }
}

/// Answers requests from recorded exchanges and logs every request it gets.
pub struct Replay {
    exchanges: Vec<(Matcher, Exchange)>,
    /// How many exchanges at the start were added with [`Replay::first`].
    overrides: usize,
    used: Mutex<Vec<bool>>,
    log: Mutex<Vec<String>>,
}

struct Matcher {
    method: String,
    url: String,
    /// The recording has no query: any query matches.
    any_query: bool,
}

impl Matcher {
    fn new(request: &str) -> Self {
        let (method, url) = request
            .split_once(' ')
            .unwrap_or_else(|| panic!("{request:?} should be `METHOD url`"));
        Self {
            method: method.to_string(),
            url: canonical_url(url, &[]),
            any_query: !url.contains('?'),
        }
    }

    fn exact(&self, method: &str, url: &str) -> bool {
        self.method == method && self.url == url
    }

    fn path(&self, method: &str, url: &str) -> bool {
        self.any_query && self.method == method && url.split('?').next() == Some(&self.url)
    }
}

impl Default for Replay {
    fn default() -> Self {
        Self::new()
    }
}

impl Replay {
    pub fn new() -> Self {
        Self {
            exchanges: Vec::new(),
            overrides: 0,
            used: Mutex::new(Vec::new()),
            log: Mutex::new(Vec::new()),
        }
    }

    /// The cassette `tests/fixtures/cassettes/<name>.json`.
    pub fn cassette(name: &str) -> Self {
        Self::from_file(&cassette_path(name))
    }

    pub fn from_file(path: &Path) -> Self {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cassette {}: {e}", path.display()));
        let cassette: Cassette = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("cassette {}: {e}", path.display()));
        cassette.exchanges.into_iter().fold(Self::new(), Self::with)
    }

    /// Adds an exchange. Several for the same request are answered in order; the last
    /// one repeats.
    pub fn with(mut self, exchange: Exchange) -> Self {
        self.exchanges
            .push((Matcher::new(&exchange.request), exchange));
        self.used
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .push(false);
        self
    }

    /// Adds an exchange that is answered once before any other match, even an exact
    /// one (an error in place of a recorded success, say). Several are used in order.
    pub fn first(mut self, exchange: Exchange) -> Self {
        self.exchanges
            .insert(self.overrides, (Matcher::new(&exchange.request), exchange));
        self.overrides += 1;
        self.used
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .push(false);
        self
    }

    /// Every request answered so far, as `METHOD url` with the query sorted.
    pub fn requests(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn answer(&self, request: &Request) -> Result<Response, String> {
        let key = request.key();
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(key.clone());
        let (method, url) = key.split_once(' ').expect("keys are `METHOD url`");
        let mut used = self.used.lock().unwrap_or_else(|e| e.into_inner());
        let unused_override = (0..self.overrides).find(|&i| {
            let m = &self.exchanges[i].0;
            !used[i] && (m.exact(method, url) || m.path(method, url))
        });
        let mut matches: Vec<usize> = self.indices(|m| m.exact(method, url));
        if matches.is_empty() {
            matches = self.indices(|m| m.path(method, url));
        }
        // A used override isn't answered again.
        matches.retain(|&i| i >= self.overrides);
        let matches: Vec<usize> = unused_override.into_iter().chain(matches).collect();
        let Some(index) = matches
            .iter()
            .copied()
            .find(|&i| !used[i])
            .or_else(|| matches.last().copied())
        else {
            panic!(
                "no recorded response for `{key}`. Record it (EDDY_RECORD=1 with the live \
                 test) or add it to the cassette."
            );
        };
        used[index] = true;
        let exchange = &self.exchanges[index].1;
        if let Some(message) = &exchange.transport_error {
            return Err(message.clone());
        }
        Ok(Response {
            status: exchange.status,
            location: exchange.location.clone(),
            body: exchange.body(),
        })
    }

    fn indices(&self, f: impl Fn(&Matcher) -> bool) -> Vec<usize> {
        self.exchanges
            .iter()
            .enumerate()
            .filter(|(_, (m, _))| f(m))
            .map(|(i, _)| i)
            .collect()
    }
}

/// For live tests: records to the cassette `name` when `EDDY_RECORD` is set, so the
/// offline test with the same cassette gets fresh responses.
#[cfg(test)]
pub(crate) fn live(name: &str) -> crate::http::Http {
    if std::env::var_os("EDDY_RECORD").is_some() {
        crate::http::Http::recording(name)
    } else {
        crate::http::Http::new()
    }
}

/// Answers from the cassette `name`, and returns the replay to inspect its requests.
#[cfg(test)]
pub(crate) fn offline(name: &str) -> (crate::http::Http, std::sync::Arc<Replay>) {
    replaying(Replay::cassette(name))
}

#[cfg(test)]
pub(crate) fn replaying(replay: Replay) -> (crate::http::Http, std::sync::Arc<Replay>) {
    let replay = std::sync::Arc::new(replay);
    (crate::http::Http::replay(replay.clone()), replay)
}

/// Saves live exchanges to a cassette, with secrets removed.
#[cfg(test)]
pub(crate) struct Recorder {
    path: PathBuf,
    cassette: Mutex<Cassette>,
}

#[cfg(test)]
impl Recorder {
    pub fn new(name: &str) -> Self {
        Self {
            path: cassette_path(name),
            cassette: Mutex::new(Cassette {
                note: format!(
                    "Recorded {} with `EDDY_RECORD=1 cargo test -- --ignored`.",
                    today()
                ),
                exchanges: Vec::new(),
            }),
        }
    }

    pub fn record(&self, request: &Request, response: &Response) {
        let body = redact_text(&trim_html(&response.body));
        let mut exchange = match serde_json::from_str::<Value>(&body) {
            Ok(mut json) => {
                redact_json(&mut json);
                Exchange::json(&request.key(), json)
            }
            Err(_) => Exchange::text(&request.key(), &body),
        };
        exchange.status = response.status;
        exchange.location.clone_from(&response.location);
        let mut cassette = self.cassette.lock().unwrap_or_else(|e| e.into_inner());
        cassette.exchanges.push(exchange);
        let text = serde_json::to_string_pretty(&*cassette).expect("serializable");
        std::fs::create_dir_all(self.path.parent().expect("in a directory")).expect("fixtures dir");
        std::fs::write(&self.path, text + "\n").expect("cassette written");
    }
}

/// An HTML page reduced to its `__NEXT_DATA__` script (all the Spotify embed parser
/// reads), so cassettes stay small. Other bodies are unchanged.
#[cfg(test)]
fn trim_html(body: &str) -> String {
    let Some(start) = body.find(r#"<script id="__NEXT_DATA__""#) else {
        return body.to_string();
    };
    let Some(end) = body[start..].find("</script>") else {
        return body.to_string();
    };
    format!(
        "<html><body>{}</script></body></html>",
        &body[start..start + end]
    )
}

#[cfg(test)]
fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if SECRET_KEYS.contains(&key.as_str()) && value.is_string() {
                    *value = Value::String(REDACTED.into());
                } else {
                    redact_json(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_json),
        _ => {}
    }
}

/// Replaces `"<secret key>":"…"` values in non-JSON bodies (JSON inside HTML).
#[cfg(test)]
fn redact_text(body: &str) -> String {
    let mut out = body.to_string();
    for (start, end) in secret_values(body).into_iter().rev() {
        out.replace_range(start..end, REDACTED);
    }
    out
}

/// Byte ranges of the values of `"<secret key>": "<value>"` in any text.
pub(crate) fn secret_values(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for key in SECRET_KEYS {
        let needle = format!("\"{key}\"");
        let mut from = 0;
        while let Some(pos) = text[from..].find(&needle) {
            let after = from + pos + needle.len();
            from = after;
            let rest = &text[after..];
            let trimmed = rest.trim_start_matches([' ', ':']);
            if !rest.trim_start().starts_with(':') || !trimmed.starts_with('"') {
                continue;
            }
            let start = after + (rest.len() - trimmed.len()) + 1;
            if let Some(len) = text[start..].find('"') {
                ranges.push((start, start + len));
            }
        }
    }
    ranges.sort();
    ranges
}

/// `YYYY-MM-DD` in UTC.
#[cfg(test)]
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn answers_in_order_and_repeats_the_last() {
        let replay = Replay::new()
            .with(Exchange::status("GET https://example.org/a", 503))
            .with(Exchange::text("GET https://example.org/a", "ok"));
        let request = Request::get("https://example.org/a");
        assert_eq!(replay.answer(&request).unwrap().status, 503);
        assert_eq!(replay.answer(&request).unwrap().body, "ok");
        assert_eq!(replay.answer(&request).unwrap().body, "ok");
        assert_eq!(replay.requests().len(), 3);
    }

    #[test]
    fn exact_queries_win_over_path_matches() {
        let replay = Replay::new()
            .with(Exchange::text("GET https://example.org/s", "any"))
            .with(Exchange::text("GET https://example.org/s?q=b&p=1", "exact"));
        let exact = Request::get("https://example.org/s")
            .query("p", "1")
            .query("q", "b");
        let other = Request::get("https://example.org/s").query("q", "c");
        assert_eq!(replay.answer(&exact).unwrap().body, "exact");
        assert_eq!(replay.answer(&other).unwrap().body, "any");
    }

    #[test]
    fn overrides_win_once() {
        let replay = Replay::new()
            .with(Exchange::text("GET https://example.org/s?q=a", "recorded"))
            .first(Exchange::status("GET https://example.org/s", 500));
        let request = Request::get("https://example.org/s").query("q", "a");
        assert_eq!(replay.answer(&request).unwrap().status, 500);
        assert_eq!(replay.answer(&request).unwrap().body, "recorded");
        assert_eq!(replay.answer(&request).unwrap().body, "recorded");
    }

    #[test]
    #[should_panic(expected = "no recorded response for `GET https://example.org/missing`")]
    fn unrecorded_requests_fail_the_test() {
        let _ = Replay::new().answer(&Request::get("https://example.org/missing"));
    }

    #[test]
    fn redacts_tokens() {
        let mut value =
            json!({"access_token": "abc", "nested": [{"refresh_token": "def"}], "title": "x"});
        redact_json(&mut value);
        assert_eq!(
            value,
            json!({"access_token": REDACTED, "nested": [{"refresh_token": REDACTED}], "title": "x"})
        );
        assert_eq!(
            redact_text(r#"<script>{"accessToken":"BQabc","x":1, "clientToken" : "zz"}</script>"#),
            r#"<script>{"accessToken":"REDACTED","x":1, "clientToken" : "REDACTED"}</script>"#
        );
    }

    #[test]
    fn computes_dates() {
        let date = today();
        assert_eq!(date.len(), 10);
        assert!(date.as_str() >= "2026-01-01", "{date}");
    }

    /// Fixtures are public: no token, secret or Authorization header may be in them.
    #[test]
    fn fixtures_contain_no_secrets() {
        let mut files = Vec::new();
        let mut dirs = vec![fixtures_dir()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        assert!(files.len() > 5, "{files:?}");
        for path in files {
            let raw = std::fs::read_to_string(&path).unwrap();
            // Bodies inside cassettes are JSON strings, with their quotes escaped.
            let mut texts = vec![raw.clone()];
            if let Ok(json) = serde_json::from_str::<Value>(&raw) {
                collect_strings(&json, &mut texts);
            }
            for text in texts {
                for marker in ["Bearer ", "Basic ", "client_secret", "Authorization"] {
                    assert!(
                        !text.contains(marker),
                        "{} contains {marker:?}",
                        path.display()
                    );
                }
                for (start, end) in secret_values(&text) {
                    assert_eq!(
                        &text[start..end],
                        REDACTED,
                        "{} has an unredacted token",
                        path.display()
                    );
                }
            }
        }
    }

    fn collect_strings(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(s) => out.push(s.clone()),
            Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
            Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
            _ => {}
        }
    }
}
