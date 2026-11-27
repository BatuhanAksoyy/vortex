//! The page's requests, as the browser reports them: what was asked for,
//! what came back, how big and how long.

use std::collections::HashMap;

use anyhow::Context as _;
use base64::Engine as _;
use serde_json::{Value, json};

use crate::Page;

/// The newest ones; a page can make any number.
const MOST: usize = 1000;
/// Bodies larger than this are cut.
const MAX_BODY: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum NetworkEvent {
    Sent {
        id: String,
        url: String,
        method: String,
        kind: String,
        /// In seconds, from a clock of the browser's.
        at: f64,
        headers: Vec<(String, String)>,
        /// Ended the request before this one, of the same id.
        redirected: Option<Response>,
    },
    Received {
        id: String,
        response: Response,
    },
    Finished {
        id: String,
        at: f64,
        size: u64,
    },
    Failed {
        id: String,
        at: f64,
        error: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub status_text: String,
    pub mime: String,
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkRequest {
    pub id: String,
    pub url: String,
    pub method: String,
    /// `Document`, `Script`, `Fetch`…, as the browser names it.
    pub kind: String,
    pub headers: Vec<(String, String)>,
    pub response: Option<Response>,
    /// Bytes received, once finished.
    pub size: Option<u64>,
    pub started: f64,
    pub ended: Option<f64>,
    pub failed: Option<String>,
}

impl NetworkRequest {
    /// What a list shows it as: the file, or the site.
    #[must_use]
    pub fn name(&self) -> &str {
        let path = self.url.split(['?', '#']).next().unwrap_or_default();
        let path = path.trim_end_matches('/');
        let last = path.rsplit('/').next().unwrap_or_default();
        if last.is_empty() { &self.url } else { last }
    }

    /// How long it took, in milliseconds.
    #[must_use]
    pub fn duration(&self) -> Option<f64> {
        self.ended.map(|ended| (ended - self.started) * 1000.)
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.ended.is_some()
    }
}

/// The requests of the document shown, in the order they were made.
#[derive(Debug, Default)]
pub struct NetworkLog {
    requests: Vec<NetworkRequest>,
    by_id: HashMap<String, usize>,
    /// Requests that were redirected, so each step keeps a row.
    redirects: usize,
}

impl NetworkLog {
    pub fn apply(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::Sent {
                id,
                url,
                method,
                kind,
                at,
                headers,
                redirected,
            } => {
                if let Some(response) = redirected {
                    self.end_redirect(&id, response, at);
                }
                self.add(NetworkRequest {
                    id,
                    url,
                    method,
                    kind,
                    headers,
                    started: at,
                    ..NetworkRequest::default()
                });
            }
            NetworkEvent::Received { id, response } => {
                if let Some(request) = self.find(&id) {
                    request.response = Some(response);
                }
            }
            NetworkEvent::Finished { id, at, size } => {
                if let Some(request) = self.find(&id) {
                    request.ended = Some(at);
                    request.size = Some(size);
                }
            }
            NetworkEvent::Failed { id, at, error } => {
                if let Some(request) = self.find(&id) {
                    request.ended = Some(at);
                    request.failed = Some(error);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.requests.clear();
        self.by_id.clear();
    }

    #[must_use]
    pub fn requests(&self) -> &[NetworkRequest] {
        &self.requests
    }

    fn find(&mut self, id: &str) -> Option<&mut NetworkRequest> {
        let at = *self.by_id.get(id)?;
        self.requests.get_mut(at)
    }

    /// The step before a redirect keeps its row, under an id of its own.
    fn end_redirect(&mut self, id: &str, response: Response, at: f64) {
        self.redirects += 1;
        let step = format!("{id}:{}", self.redirects);
        let Some(at_row) = self.by_id.remove(id) else {
            return;
        };
        self.by_id.insert(step.clone(), at_row);
        if let Some(request) = self.requests.get_mut(at_row) {
            request.id = step;
            request.response = Some(response);
            request.ended = Some(at);
        }
    }

    fn add(&mut self, request: NetworkRequest) {
        if self.requests.len() == MOST {
            self.requests.drain(..MOST / 10);
            self.by_id = (self.requests.iter().enumerate())
                .map(|(at, request)| (request.id.clone(), at))
                .collect();
        }
        self.by_id.insert(request.id.clone(), self.requests.len());
        self.requests.push(request);
    }
}

pub(crate) fn parse(method: &str, params: &Value) -> Option<NetworkEvent> {
    let id = || params["requestId"].as_str().map(str::to_owned);
    let at = params["timestamp"].as_f64().unwrap_or_default();
    Some(match method {
        "Network.requestWillBeSent" => {
            let request = &params["request"];
            NetworkEvent::Sent {
                id: id()?,
                url: request["url"].as_str()?.to_owned(),
                method: request["method"].as_str().unwrap_or("GET").to_owned(),
                kind: params["type"].as_str().unwrap_or("Other").to_owned(),
                at,
                headers: headers(&request["headers"]),
                redirected: params.get("redirectResponse").map(response),
            }
        }
        "Network.responseReceived" => NetworkEvent::Received {
            id: id()?,
            response: response(&params["response"]),
        },
        "Network.loadingFinished" => NetworkEvent::Finished {
            id: id()?,
            at,
            size: params["encodedDataLength"].as_f64().map_or(0, |size| {
                // Whole bytes, never negative.
                format!("{size:.0}").parse().unwrap_or(0)
            }),
        },
        "Network.loadingFailed" => NetworkEvent::Failed {
            id: id()?,
            at,
            error: params["errorText"].as_str().unwrap_or("Failed").to_owned(),
        },
        _ => return None,
    })
}

fn response(response: &Value) -> Response {
    let status = response["status"].as_u64().unwrap_or_default();
    Response {
        status: u16::try_from(status).unwrap_or_default(),
        status_text: text(&response["statusText"]),
        mime: text(&response["mimeType"]),
        headers: headers(&response["headers"]),
    }
}

fn headers(headers: &Value) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = headers
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, value)| {
            (
                name.clone(),
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned),
            )
        })
        .collect();
    pairs.sort();
    pairs
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

/// A response's body: text, or how many bytes of something else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    Text { text: String, cut: bool },
    Binary(usize),
}

impl Page {
    /// What came back for request `id`, while the browser still has it.
    ///
    /// # Errors
    /// When the browser no longer has the body, or never had one.
    pub async fn response_body(&self, id: &str) -> anyhow::Result<Body> {
        let ask = json!({"requestId": id});
        let reply = self.call("Network.getResponseBody", ask).await?;
        let body = reply["body"].as_str().context("no body")?;
        let bytes = if reply["base64Encoded"] == true {
            base64::engine::general_purpose::STANDARD.decode(body)?
        } else {
            body.as_bytes().to_vec()
        };
        let size = bytes.len();
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(Body::Binary(size));
        };
        let cut = text.len() > MAX_BODY;
        let mut end = MAX_BODY.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(Body::Text {
            text: text[..end].to_owned(),
            cut,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sent(id: &str, url: &str, at: f64, redirected: Option<Response>) -> NetworkEvent {
        NetworkEvent::Sent {
            id: id.into(),
            url: url.into(),
            method: "GET".into(),
            kind: "Document".into(),
            at,
            headers: Vec::new(),
            redirected,
        }
    }

    #[test]
    fn a_request_is_followed_to_its_end() {
        let mut log = NetworkLog::default();
        log.apply(sent("1", "http://a.test/app.js?v=2", 1.0, None));
        let response = Response {
            status: 200,
            ..Response::default()
        };
        log.apply(NetworkEvent::Received {
            id: "1".into(),
            response,
        });
        log.apply(NetworkEvent::Finished {
            id: "1".into(),
            at: 1.25,
            size: 900,
        });
        let request = &log.requests()[0];
        assert_eq!(request.name(), "app.js");
        assert_eq!(request.duration(), Some(250.));
        assert_eq!(request.size, Some(900));
    }

    #[test]
    fn each_step_of_a_redirect_keeps_its_row() {
        let mut log = NetworkLog::default();
        log.apply(sent("1", "http://a.test/", 1.0, None));
        let moved = Response {
            status: 301,
            ..Response::default()
        };
        log.apply(sent("1", "https://a.test/", 1.1, Some(moved)));
        log.apply(NetworkEvent::Failed {
            id: "1".into(),
            at: 1.2,
            error: "net::ERR_ABORTED".into(),
        });
        let statuses: Vec<_> = log
            .requests()
            .iter()
            .map(|r| (r.response.as_ref().map(|r| r.status), r.failed.is_some()))
            .collect();
        assert_eq!(statuses, [(Some(301), false), (None, true)]);
        assert_eq!(log.requests()[0].name(), "a.test");
    }

    #[test]
    fn events_are_read_from_the_browser() {
        let params = json!({
            "requestId": "7",
            "timestamp": 3.5,
            "type": "Fetch",
            "request": {"url": "http://a.test/api", "method": "POST", "headers": {"B": "2", "A": "1"}},
        });
        let Some(NetworkEvent::Sent { kind, headers, .. }) =
            parse("Network.requestWillBeSent", &params)
        else {
            panic!("not sent");
        };
        assert_eq!(kind, "Fetch");
        assert_eq!(headers[0], ("A".to_owned(), "1".to_owned()));
        let finished = json!({"requestId": "7", "timestamp": 4.0, "encodedDataLength": 12.0});
        assert_eq!(
            parse("Network.loadingFinished", &finished),
            Some(NetworkEvent::Finished {
                id: "7".into(),
                at: 4.0,
                size: 12
            })
        );
    }
}
