//! WASM assertion plugin: verifies an HMAC-SHA256 signature carried in a
//! response header over the raw response body.
//!
//! Config: `{"header": "x-signature", "secret": "...", "algo": "hmac-sha256"}`.
//!
//! The verdict `pass`es when the signature recomputed from the body with the
//! shared `secret` matches the value carried in `header`. The comparison is
//! encoding-tolerant: the provided value may be lowercase/uppercase hex,
//! standard base64, or url-safe base64, and it may carry a leading
//! `sha256=` / `hmac-sha256=` label (as GitHub-style webhooks emit). The raw
//! MAC bytes are compared in constant time.
//!
//! Everything below the `wit_bindgen` glue is written as plain functions with
//! no component-model types, so the decode / verify logic is unit-testable on
//! the host (see the `tests` module).

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-assertion-plugin",
});

use exports::loadr::plugin::assertion::{Guest as Assertion, Verdict};
use exports::loadr::plugin::meta::{Guest as Meta, Info};

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Pure logic (no wit types) — unit tested on the host.
// ---------------------------------------------------------------------------

/// Plugin configuration parsed from the JSON `config` string.
#[derive(serde::Deserialize)]
struct Config {
    /// Name of the header carrying the signature (matched case-insensitively).
    header: String,
    /// Shared secret used as the HMAC key.
    secret: String,
    /// Signature algorithm. Only `hmac-sha256` is supported; when omitted it
    /// defaults to `hmac-sha256`.
    #[serde(default = "default_algo")]
    algo: String,
}

fn default_algo() -> String {
    "hmac-sha256".to_string()
}

/// Parse the JSON config, returning a human-readable error on failure.
fn parse_config(config: &str) -> Result<Config, String> {
    serde_json::from_str(config).map_err(|e| format!("invalid config: {e}"))
}

/// Whether the configured algorithm is one this plugin implements.
fn algo_supported(algo: &str) -> bool {
    algo.eq_ignore_ascii_case("hmac-sha256") || algo.eq_ignore_ascii_case("sha256")
}

/// Compute the raw HMAC-SHA256 MAC of `body` keyed by `secret`.
fn compute_hmac_sha256(secret: &[u8], body: &[u8]) -> Vec<u8> {
    // `new_from_slice` only errors for algorithms with a fixed key size;
    // HMAC accepts keys of any length, so this never fails.
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(body);
    mac.finalize().into_bytes().to_vec()
}

/// Look up a header value case-insensitively.
fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Strip a leading algorithm label (e.g. `sha256=`) from a signature value.
fn strip_algo_prefix(provided: &str) -> &str {
    for prefix in ["hmac-sha256=", "sha256="] {
        if provided.len() >= prefix.len() && provided[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return &provided[prefix.len()..];
        }
    }
    provided
}

/// Decode a hex string into bytes, or `None` if it is not valid hex.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = (bytes[i] as char).to_digit(16)?;
        let lo = (bytes[i + 1] as char).to_digit(16)?;
        out.push(((hi << 4) | lo) as u8);
        i += 2;
    }
    Some(out)
}

/// Constant-time byte-slice equality (length is not secret; contents are).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Decode a provided signature (hex or base64, any label stripped) into raw
/// MAC bytes, then compare against `computed` in constant time.
fn signatures_match(computed: &[u8], provided: &str) -> bool {
    let provided = strip_algo_prefix(provided.trim());
    // Hex is tried first: a 64-char hex digest is also valid base64, but hex
    // decoding yields the correct 32 bytes whereas a base64 read would not.
    if let Some(bytes) = decode_hex(provided) {
        if constant_time_eq(&bytes, computed) {
            return true;
        }
    }
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(provided) {
        if constant_time_eq(&bytes, computed) {
            return true;
        }
    }
    if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(provided) {
        if constant_time_eq(&bytes, computed) {
            return true;
        }
    }
    false
}

/// Full verification pipeline: parse config, locate the header, recompute the
/// MAC and compare. Returns `(pass, detail)` with wit types kept out so the
/// host can exercise it directly.
fn run_check(body: &[u8], headers: &[(String, String)], config: &str) -> (bool, String) {
    let cfg = match parse_config(config) {
        Ok(c) => c,
        Err(e) => return (false, e),
    };

    if !algo_supported(&cfg.algo) {
        return (
            false,
            format!(
                "unsupported algo {:?}: only hmac-sha256 is supported",
                cfg.algo
            ),
        );
    }

    let provided = match find_header(headers, &cfg.header) {
        Some(v) => v,
        None => {
            return (
                false,
                format!("signature header {:?} not present in response", cfg.header),
            )
        }
    };

    if provided.trim().is_empty() {
        return (false, format!("signature header {:?} is empty", cfg.header));
    }

    let computed = compute_hmac_sha256(cfg.secret.as_bytes(), body);

    if signatures_match(&computed, provided) {
        (
            true,
            format!(
                "hmac-sha256 signature in {:?} matches the response body",
                cfg.header
            ),
        )
    } else {
        (
            false,
            format!(
                "hmac-sha256 signature in {:?} does not match the response body",
                cfg.header
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Component glue.
// ---------------------------------------------------------------------------

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "response-signature".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "assertion".to_string(),
            description: "Verifies an HMAC-SHA256 signature header computed over the response body"
                .to_string(),
        }
    }
}

impl Assertion for Plugin {
    fn check(
        _status: i64,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
        _duration_ms: f64,
        config: String,
    ) -> Verdict {
        let (pass, detail) = run_check(&body, &headers, &config);
        Verdict { pass, detail }
    }
}

export!(Plugin);

// ---------------------------------------------------------------------------
// Host-side unit tests for the pure logic.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    #[test]
    fn hmac_matches_known_vector() {
        // RFC 4231 test case 2: key = "Jefe", data = "what do ya want ...".
        let mac = compute_hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            hex(&mac),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn decode_hex_roundtrip() {
        assert_eq!(decode_hex("00ff10").unwrap(), vec![0x00, 0xff, 0x10]);
        assert_eq!(
            decode_hex("DEADbeef").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
    }

    #[test]
    fn decode_hex_rejects_bad_input() {
        assert!(decode_hex("abc").is_none()); // odd length
        assert!(decode_hex("zz").is_none()); // non-hex
        assert!(decode_hex("").is_none()); // empty
    }

    #[test]
    fn constant_time_eq_basics() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn strip_algo_prefix_variants() {
        assert_eq!(strip_algo_prefix("sha256=deadbeef"), "deadbeef");
        assert_eq!(strip_algo_prefix("SHA256=deadbeef"), "deadbeef");
        assert_eq!(strip_algo_prefix("hmac-sha256=deadbeef"), "deadbeef");
        assert_eq!(strip_algo_prefix("deadbeef"), "deadbeef");
    }

    #[test]
    fn signatures_match_hex() {
        let mac = compute_hmac_sha256(b"topsecret", b"hello world");
        assert!(signatures_match(&mac, &hex(&mac)));
        assert!(signatures_match(&mac, &hex(&mac).to_uppercase()));
        assert!(signatures_match(&mac, &format!("sha256={}", hex(&mac))));
    }

    #[test]
    fn signatures_match_base64() {
        let mac = compute_hmac_sha256(b"topsecret", b"hello world");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&mac);
        assert!(signatures_match(&mac, &b64));
        let b64url = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&mac);
        assert!(signatures_match(&mac, &b64url));
    }

    #[test]
    fn signatures_reject_wrong() {
        let mac = compute_hmac_sha256(b"topsecret", b"hello world");
        let wrong = compute_hmac_sha256(b"topsecret", b"goodbye world");
        assert!(!signatures_match(&mac, &hex(&wrong)));
        assert!(!signatures_match(&mac, "not-a-signature"));
    }

    fn cfg(header: &str, secret: &str) -> String {
        format!(r#"{{"header":"{header}","secret":"{secret}","algo":"hmac-sha256"}}"#)
    }

    #[test]
    fn run_check_passes_on_valid_signature() {
        let body = b"{\"ok\":true}";
        let mac = compute_hmac_sha256(b"s3cr3t", body);
        let headers = vec![("X-Signature".to_string(), hex(&mac))];
        let (pass, detail) = run_check(body, &headers, &cfg("x-signature", "s3cr3t"));
        assert!(pass, "detail: {detail}");
    }

    #[test]
    fn run_check_case_insensitive_header() {
        let body = b"payload";
        let mac = compute_hmac_sha256(b"key", body);
        // Header name in config differs in case from the actual header.
        let headers = vec![("x-SIGNATURE".to_string(), hex(&mac))];
        let (pass, _) = run_check(body, &headers, &cfg("X-Signature", "key"));
        assert!(pass);
    }

    #[test]
    fn run_check_fails_on_tampered_body() {
        let mac = compute_hmac_sha256(b"s3cr3t", b"original");
        let headers = vec![("x-signature".to_string(), hex(&mac))];
        let (pass, detail) = run_check(b"tampered", &headers, &cfg("x-signature", "s3cr3t"));
        assert!(!pass);
        assert!(detail.contains("does not match"));
    }

    #[test]
    fn run_check_fails_on_wrong_secret() {
        let body = b"body";
        let mac = compute_hmac_sha256(b"right", body);
        let headers = vec![("x-signature".to_string(), hex(&mac))];
        let (pass, _) = run_check(body, &headers, &cfg("x-signature", "wrong"));
        assert!(!pass);
    }

    #[test]
    fn run_check_missing_header() {
        let (pass, detail) = run_check(b"body", &[], &cfg("x-signature", "s3cr3t"));
        assert!(!pass);
        assert!(detail.contains("not present"));
    }

    #[test]
    fn run_check_empty_header_value() {
        let headers = vec![("x-signature".to_string(), "   ".to_string())];
        let (pass, detail) = run_check(b"body", &headers, &cfg("x-signature", "s3cr3t"));
        assert!(!pass);
        assert!(detail.contains("empty"));
    }

    #[test]
    fn run_check_unsupported_algo() {
        let config = r#"{"header":"x-signature","secret":"s","algo":"hmac-sha512"}"#;
        let headers = vec![("x-signature".to_string(), "abcd".to_string())];
        let (pass, detail) = run_check(b"body", &headers, config);
        assert!(!pass);
        assert!(detail.contains("unsupported algo"));
    }

    #[test]
    fn run_check_default_algo_when_omitted() {
        let body = b"data";
        let mac = compute_hmac_sha256(b"k", body);
        let headers = vec![("x-signature".to_string(), hex(&mac))];
        let config = r#"{"header":"x-signature","secret":"k"}"#;
        let (pass, _) = run_check(body, &headers, config);
        assert!(pass);
    }

    #[test]
    fn run_check_invalid_config() {
        let (pass, detail) = run_check(b"body", &[], "not json");
        assert!(!pass);
        assert!(detail.contains("invalid config"));
    }
}
