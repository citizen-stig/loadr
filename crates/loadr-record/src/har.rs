//! Accumulates captured transactions as a HAR 1.2 document, the exact shape
//! [`loadr_convert::convert_har`] consumes (with its heuristic auto-correlation).

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

/// One captured request/response pair, ready to become a HAR entry.
pub struct Captured {
    pub method: String,
    pub url: String,
    pub http_version: String,
    pub req_headers: Vec<(String, String)>,
    pub req_body: Option<(String, String)>, // (mime, text)
    pub status: u16,
    pub status_text: String,
    pub resp_headers: Vec<(String, String)>,
    pub resp_mime: String,
    pub resp_body: Option<String>,
    pub wait_ms: f64,
}

/// Thread-safe, append-only capture log shared across proxy connections.
#[derive(Clone, Default)]
pub struct Recording {
    entries: Arc<Mutex<Vec<Value>>>,
}

impl Recording {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of transactions captured so far.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Record one transaction.
    pub fn push(&self, c: Captured) {
        let entry = json!({
            "startedDateTime": "1970-01-01T00:00:00.000Z",
            "time": c.wait_ms,
            "request": {
                "method": c.method,
                "url": c.url,
                "httpVersion": c.http_version,
                "headers": header_array(&c.req_headers),
                "queryString": [],
                "cookies": [],
                "headersSize": -1,
                "bodySize": c.req_body.as_ref().map(|(_, t)| t.len() as i64).unwrap_or(-1),
                "postData": c.req_body.map(|(mime, text)| json!({
                    "mimeType": mime,
                    "text": text,
                })),
            },
            "response": {
                "status": c.status,
                "statusText": c.status_text,
                "httpVersion": c.http_version,
                "headers": header_array(&c.resp_headers),
                "cookies": [],
                "redirectURL": "",
                "headersSize": -1,
                "bodySize": c.resp_body.as_ref().map(|b| b.len() as i64).unwrap_or(-1),
                "content": {
                    "mimeType": c.resp_mime,
                    "size": c.resp_body.as_ref().map(|b| b.len() as i64).unwrap_or(0),
                    "text": c.resp_body,
                },
            },
            "cache": {},
            "timings": { "send": 0, "wait": c.wait_ms, "receive": 0 },
        });
        self.entries.lock().unwrap().push(entry);
    }

    /// Serialize the recording as a HAR 1.2 JSON document.
    pub fn to_har(&self) -> String {
        let entries = self.entries.lock().unwrap().clone();
        let doc = json!({
            "log": {
                "version": "1.2",
                "creator": { "name": "loadr record", "version": env!("CARGO_PKG_VERSION") },
                "entries": entries,
            }
        });
        serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
    }
}

fn header_array(headers: &[(String, String)]) -> Value {
    Value::Array(
        headers
            .iter()
            .map(|(n, v)| json!({ "name": n, "value": v }))
            .collect(),
    )
}
