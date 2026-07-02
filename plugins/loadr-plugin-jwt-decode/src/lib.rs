//! WASM extractor plugin: decodes a JWT and extracts a named claim from the
//! payload segment as a string.
//!
//! Config: `{"claim": "sub"}`.
//!
//! The token is taken from the response body (UTF-8). It is split on `.`, the
//! second segment (payload) is base64url-decoded (no padding), parsed as JSON,
//! and the requested claim is returned. Signatures are NOT verified — this is a
//! decode-only extractor.

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-plugin",
});

use exports::loadr::plugin::extractor::Guest as Extractor;
use exports::loadr::plugin::meta::{Guest as Meta, Info};

use base64::Engine;

#[derive(serde::Deserialize)]
struct Config {
    claim: String,
}

// ---------------------------------------------------------------------------
// Pure logic (no wit types) — unit-tested on the host below.
// ---------------------------------------------------------------------------

/// Base64url-decode a JWT segment. JWT segments use the URL-safe alphabet and
/// omit padding, so pad back to a multiple of 4 before decoding.
fn base64url_decode(segment: &str) -> Option<Vec<u8>> {
    let mut s = segment.to_string();
    let rem = s.len() % 4;
    if rem != 0 {
        s.push_str(&"=".repeat(4 - rem));
    }
    base64::engine::general_purpose::URL_SAFE
        .decode(s.as_bytes())
        .ok()
}

/// Decode the payload segment of a JWT into a JSON value.
fn decode_payload(token: &str) -> Option<serde_json::Value> {
    let token = token.trim();
    // A JWT is header.payload.signature — the payload is the 2nd segment.
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    if payload.is_empty() {
        return None;
    }
    let bytes = base64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

/// Render a JSON claim value as a string. Strings are returned verbatim;
/// numbers/booleans are stringified; arrays/objects are re-serialized to JSON;
/// null yields `None`.
fn claim_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        other => serde_json::to_string(other).ok(),
    }
}

/// Decode `token` and extract `claim` from its payload as a string.
fn extract_claim(token: &str, claim: &str) -> Option<String> {
    let payload = decode_payload(token)?;
    let value = payload.get(claim)?;
    claim_to_string(value)
}

// ---------------------------------------------------------------------------
// WASM plugin exports.
// ---------------------------------------------------------------------------

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "jwt-decode".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "extractor".to_string(),
            description: "Decodes a JWT and extracts the named claim from the payload".to_string(),
        }
    }
}

impl Extractor for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let config: Config = serde_json::from_str(&config).ok()?;
        if config.claim.is_empty() {
            return None;
        }
        let token = String::from_utf8_lossy(&body);
        extract_claim(&token, &config.claim)
    }
}

export!(Plugin);

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: build a JWT with an unsigned/dummy signature from a payload JSON.
    fn make_token(payload: &str) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let body = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("{header}.{body}.sig")
    }

    #[test]
    fn extracts_string_claim() {
        let token = make_token(r#"{"sub":"1234567890","name":"Jane Doe"}"#);
        assert_eq!(extract_claim(&token, "sub"), Some("1234567890".to_string()));
        assert_eq!(extract_claim(&token, "name"), Some("Jane Doe".to_string()));
    }

    #[test]
    fn extracts_numeric_and_bool_claims_as_strings() {
        let token = make_token(r#"{"iat":1516239022,"admin":true}"#);
        assert_eq!(extract_claim(&token, "iat"), Some("1516239022".to_string()));
        assert_eq!(extract_claim(&token, "admin"), Some("true".to_string()));
    }

    #[test]
    fn extracts_nested_claim_as_json() {
        let token = make_token(r#"{"roles":["a","b"],"meta":{"x":1}}"#);
        assert_eq!(
            extract_claim(&token, "roles"),
            Some(r#"["a","b"]"#.to_string())
        );
        assert_eq!(
            extract_claim(&token, "meta"),
            Some(r#"{"x":1}"#.to_string())
        );
    }

    #[test]
    fn missing_claim_returns_none() {
        let token = make_token(r#"{"sub":"abc"}"#);
        assert_eq!(extract_claim(&token, "email"), None);
    }

    #[test]
    fn null_claim_returns_none() {
        let token = make_token(r#"{"sub":null}"#);
        assert_eq!(extract_claim(&token, "sub"), None);
    }

    #[test]
    fn known_jwt_io_sample_decodes() {
        // The canonical jwt.io example token.
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.\
            eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.\
            SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        assert_eq!(extract_claim(token, "sub"), Some("1234567890".to_string()));
        assert_eq!(extract_claim(token, "name"), Some("John Doe".to_string()));
        assert_eq!(extract_claim(token, "iat"), Some("1516239022".to_string()));
    }

    #[test]
    fn base64url_decode_pads_correctly() {
        // "sub" payload without padding decodes cleanly.
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{\"a\":1}");
        assert_eq!(base64url_decode(&raw), Some(b"{\"a\":1}".to_vec()));
    }

    #[test]
    fn malformed_tokens_return_none() {
        assert_eq!(extract_claim("not-a-jwt", "sub"), None);
        assert_eq!(extract_claim("only.two", "sub"), None); // payload "two" is not valid base64url JSON
        assert_eq!(extract_claim("header..sig", "sub"), None); // empty payload
        assert_eq!(extract_claim("", "sub"), None);
    }
}
