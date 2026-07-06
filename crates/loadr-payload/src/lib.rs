//! Adversarial payload generator.
//!
//! Real systems rarely fall over on *typical* input — they fall over on input
//! crafted to hit a super-linear code path: a deeply-nested document that makes
//! a parser go quadratic, an expansion bomb that turns a kilobyte into a
//! gigabyte, a string that sends a validator's regex into catastrophic
//! backtracking. This crate generates those inputs, parameterised by a single
//! magnitude (depth / count / bytes / levels) so a caller can *scale* them and
//! watch for non-linear response-time growth.
//!
//! It is pure and deterministic: `generate` takes a [`PayloadSpec`] and returns
//! bytes, with no I/O, no randomness and a hard safety cap per kind so a
//! generator can never be asked to allocate the machine to death.
//!
//! ```
//! use loadr_payload::generate_str;
//! let bytes = generate_str("nested-json:8").unwrap();
//! assert_eq!(bytes, br#"{"a":{"a":{"a":{"a":{"a":{"a":{"a":{"a":1}}}}}}}}"#);
//! ```

use std::fmt::Write as _;

/// Error raised while parsing a spec or generating a payload.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PayloadError {
    /// The `name` part of a spec is not a known payload kind.
    #[error("unknown payload `{0}` — run `loadr payload --list` to see the catalog")]
    Unknown(String),
    /// The `:magnitude` part could not be parsed as a non-negative integer.
    #[error("invalid magnitude `{0}` in payload spec (expected a whole number)")]
    BadMagnitude(String),
    /// The requested magnitude exceeds the kind's safety cap.
    #[error("payload `{name}` magnitude {got} exceeds the safety cap of {max} (the {param})")]
    TooLarge {
        /// Payload kind name.
        name: String,
        /// The requested magnitude.
        got: u64,
        /// The kind's cap.
        max: u64,
        /// What the magnitude means for this kind (depth/count/bytes/levels).
        param: &'static str,
    },
}

/// Static description of one payload kind — powers `--list` and the docs.
#[derive(Debug, Clone, Copy)]
pub struct PayloadInfo {
    /// Spec name (the part before the `:`).
    pub name: &'static str,
    /// Grouping for listings: `nesting`, `amplification`, `volume`, `regex`,
    /// `unicode`, `numeric`, `collision`.
    pub category: &'static str,
    /// What the magnitude controls: `depth`, `count`, `bytes`, `levels`.
    pub param: &'static str,
    /// Magnitude used when the spec omits `:n`.
    pub default: u64,
    /// Hard safety cap on the magnitude.
    pub max: u64,
    /// Suggested `Content-Type` for a request carrying this body.
    pub content_type: &'static str,
    /// One-line description of what it stresses.
    pub about: &'static str,
}

/// The full catalog of payload kinds.
pub const CATALOG: &[PayloadInfo] = &[
    // ---- nesting: deep structure → super-linear parsers -------------------
    PayloadInfo { name: "nested-json", category: "nesting", param: "depth", default: 10_000, max: 5_000_000, content_type: "application/json",
        about: "Deeply nested JSON object {\"a\":{\"a\":…}} — stresses recursive-descent / stack-depth parsing." },
    PayloadInfo { name: "nested-array", category: "nesting", param: "depth", default: 10_000, max: 5_000_000, content_type: "application/json",
        about: "Deeply nested JSON array [[[…]]] — same parser stress via array nesting." },
    PayloadInfo { name: "nested-markdown-blockquote", category: "nesting", param: "depth", default: 50_000, max: 5_000_000, content_type: "text/markdown",
        about: "One line of N blockquote markers (>>>…) — the goldmark-class super-quadratic blowup." },
    PayloadInfo { name: "nested-markdown-bracket", category: "nesting", param: "depth", default: 50_000, max: 5_000_000, content_type: "text/markdown",
        about: "Unmatched nested link brackets [[[…]]] — stresses inline link/reference backtracking." },
    PayloadInfo { name: "nested-xml", category: "nesting", param: "depth", default: 20_000, max: 5_000_000, content_type: "application/xml",
        about: "Deeply nested XML elements <a><a>…</a></a> — stack/tree-depth parser stress." },
    PayloadInfo { name: "nested-html", category: "nesting", param: "depth", default: 20_000, max: 5_000_000, content_type: "text/html",
        about: "Deeply nested <div> tags — stresses HTML parsers and sanitizers walking a deep tree." },
    PayloadInfo { name: "nested-parens", category: "nesting", param: "depth", default: 50_000, max: 5_000_000, content_type: "text/plain",
        about: "Balanced nested parentheses ((((…)))) — stresses expression/formula/filter grammars." },
    PayloadInfo { name: "nested-graphql", category: "nesting", param: "depth", default: 2_000, max: 200_000, content_type: "application/json",
        about: "Deeply nested GraphQL selection {a{a{…}}} — stresses query validation / depth limiting." },
    // ---- amplification: small in → huge out -------------------------------
    PayloadInfo { name: "billion-laughs", category: "amplification", param: "levels", default: 9, max: 12, content_type: "application/xml",
        about: "Classic XML entity-expansion bomb — ~10^levels expansion from a tiny document." },
    PayloadInfo { name: "yaml-alias-bomb", category: "amplification", param: "levels", default: 10, max: 24, content_type: "application/x-yaml",
        about: "Exponential YAML anchor/alias expansion (&a […] then [*a,*a] …) — 2^levels blowup." },
    // ---- volume: allocation / O(n^2) stress -------------------------------
    PayloadInfo { name: "json-array", category: "volume", param: "count", default: 1_000_000, max: 50_000_000, content_type: "application/json",
        about: "A flat JSON array of N integers — allocation, GC and per-element processing stress." },
    PayloadInfo { name: "json-object-keys", category: "volume", param: "count", default: 1_000_000, max: 20_000_000, content_type: "application/json",
        about: "A JSON object with N distinct keys — hashmap-build and key-processing stress." },
    PayloadInfo { name: "long-string", category: "volume", param: "bytes", default: 10_000_000, max: 200_000_000, content_type: "application/json",
        about: "A single JSON string of N bytes — copy/scan/validation cost in one enormous field." },
    PayloadInfo { name: "csv-rows", category: "volume", param: "count", default: 1_000_000, max: 50_000_000, content_type: "text/csv",
        about: "A CSV with N rows — row-parsing throughput and streaming behaviour." },
    // ---- regex: catastrophic backtracking (ReDoS) -------------------------
    PayloadInfo { name: "redos", category: "regex", param: "bytes", default: 50_000, max: 10_000_000, content_type: "text/plain",
        about: "'aaaa…!' — drives (a+)+$-style vulnerable validators into exponential backtracking." },
    // ---- unicode: normalization / grapheme cost ---------------------------
    PayloadInfo { name: "zalgo", category: "unicode", param: "count", default: 100_000, max: 20_000_000, content_type: "text/plain",
        about: "A base char with N stacked combining marks — normalization / width / grapheme cost." },
    // ---- numeric: slow number parsing -------------------------------------
    PayloadInfo { name: "bignum", category: "numeric", param: "count", default: 100_000, max: 50_000_000, content_type: "application/json",
        about: "A bare integer with N digits — bignum / arbitrary-precision parse cost." },
    // ---- collision: worst-case hashmaps -----------------------------------
    PayloadInfo { name: "hash-collision", category: "collision", param: "count", default: 65_536, max: 1_000_000, content_type: "application/json",
        about: "A JSON object whose N keys all collide in 31-based string hashing — O(n^2) map inserts." },
];

/// Look up a kind's metadata by name.
pub fn info(name: &str) -> Option<&'static PayloadInfo> {
    CATALOG.iter().find(|p| p.name == name)
}

/// A parsed `name[:magnitude]` spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadSpec {
    /// The payload kind.
    pub name: String,
    /// The magnitude (depth/count/bytes/levels).
    pub magnitude: u64,
}

/// Parse a `name` or `name:magnitude` spec, applying the kind's default and cap.
pub fn parse_spec(spec: &str) -> Result<PayloadSpec, PayloadError> {
    let (name, mag) = match spec.split_once(':') {
        Some((n, m)) => (n, Some(m)),
        None => (spec, None),
    };
    let meta = info(name).ok_or_else(|| PayloadError::Unknown(name.to_string()))?;
    let magnitude = match mag {
        None => meta.default,
        Some(m) => m
            .trim()
            .parse::<u64>()
            .map_err(|_| PayloadError::BadMagnitude(m.to_string()))?,
    };
    if magnitude > meta.max {
        return Err(PayloadError::TooLarge {
            name: name.to_string(),
            got: magnitude,
            max: meta.max,
            param: meta.param,
        });
    }
    Ok(PayloadSpec {
        name: name.to_string(),
        magnitude,
    })
}

/// Parse and generate in one call. Convenience for template use.
pub fn generate_str(spec: &str) -> Result<Vec<u8>, PayloadError> {
    generate(&parse_spec(spec)?)
}

/// Generate the payload bytes for a parsed spec.
pub fn generate(spec: &PayloadSpec) -> Result<Vec<u8>, PayloadError> {
    let n = spec.magnitude as usize;
    let s = match spec.name.as_str() {
        "nested-json" => wrap("{\"a\":", "1", "}", n),
        "nested-array" => wrap("[", "1", "]", n),
        "nested-markdown-blockquote" => ">".repeat(n) + " x",
        "nested-markdown-bracket" => "[".repeat(n) + "x" + &"]".repeat(n),
        "nested-xml" => wrap("<a>", "x", "</a>", n),
        "nested-html" => wrap("<div>", "x", "</div>", n),
        "nested-parens" => "(".repeat(n) + &")".repeat(n),
        "nested-graphql" => "{".to_string() + &"a{".repeat(n) + "id" + &"}".repeat(n) + "}",
        "billion-laughs" => billion_laughs(n),
        "yaml-alias-bomb" => yaml_alias_bomb(n),
        "json-array" => json_array(n),
        "json-object-keys" => json_object_keys(n),
        "long-string" => {
            let mut s = String::with_capacity(n + 2);
            s.push('"');
            s.extend(std::iter::repeat_n('a', n));
            s.push('"');
            s
        }
        "csv-rows" => csv_rows(n),
        "redos" => "a".repeat(n) + "!",
        "zalgo" => zalgo(n),
        "bignum" => "9".repeat(n.max(1)),
        "hash-collision" => hash_collision(n),
        other => return Err(PayloadError::Unknown(other.to_string())),
    };
    Ok(s.into_bytes())
}

/// `open`×n + `core` + `close`×n. Pre-sized to avoid reallocation on deep input.
fn wrap(open: &str, core: &str, close: &str, n: usize) -> String {
    let mut s = String::with_capacity(open.len() * n + core.len() + close.len() * n);
    for _ in 0..n {
        s.push_str(open);
    }
    s.push_str(core);
    for _ in 0..n {
        s.push_str(close);
    }
    s
}

/// The canonical "billion laughs": `levels` entities, each expanding the
/// previous ten times, so the final entity expands ~10^levels.
fn billion_laughs(levels: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\"?>\n<!DOCTYPE lolz [\n");
    s.push_str("  <!ENTITY lol0 \"loooooooooool\">\n");
    for i in 1..levels.max(1) {
        let refs = format!("&lol{};", i - 1).repeat(10);
        let _ = writeln!(s, "  <!ENTITY lol{i} \"{refs}\">");
    }
    let top = levels.max(1) - 1;
    let _ = write!(s, "]>\n<lolz>&lol{top};</lolz>");
    s
}

/// Exponential YAML: each level is an array of ten references to the previous
/// anchor, so resolving the last is 10^levels nodes.
fn yaml_alias_bomb(levels: usize) -> String {
    let levels = levels.max(1);
    let mut s = String::from("a0: &a0 \"lol\"\n");
    for i in 1..levels {
        let refs = format!("*a{}", i - 1);
        let items = std::iter::repeat_n(refs, 10).collect::<Vec<_>>().join(", ");
        let _ = writeln!(s, "a{i}: &a{i} [{items}]");
    }
    s
}

fn json_array(n: usize) -> String {
    let mut s = String::with_capacity(n * 7 + 2);
    s.push('[');
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "{i}");
    }
    s.push(']');
    s
}

fn json_object_keys(n: usize) -> String {
    let mut s = String::with_capacity(n * 12 + 2);
    s.push('{');
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "\"k{i}\":{i}");
    }
    s.push('}');
    s
}

fn csv_rows(n: usize) -> String {
    let mut s = String::with_capacity(n * 24 + 16);
    s.push_str("id,name,value\n");
    for i in 0..n {
        let _ = writeln!(s, "{i},row-{i},{}", i * 3);
    }
    s
}

/// A base letter followed by `n` combining acute accents (U+0301).
fn zalgo(n: usize) -> String {
    let mut s = String::with_capacity(1 + n * 2);
    s.push('e');
    for _ in 0..n {
        s.push('\u{0301}');
    }
    s
}

/// A JSON object with `count` keys that all share the same 31-based string hash
/// (as used by Java/many JVM langs and some JS engines). "Aa" and "BB" collide;
/// any equal-length concatenation of those two-char blocks therefore collides,
/// giving 2^k colliding keys of length 2k.
fn hash_collision(count: usize) -> String {
    let count = count.max(1);
    // Smallest k with 2^k >= count.
    let mut k = 0usize;
    while (1usize << k) < count {
        k += 1;
    }
    let mut s = String::with_capacity(count * (2 * k + 8));
    s.push('{');
    for i in 0..count {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        for bit in (0..k).rev() {
            s.push_str(if (i >> bit) & 1 == 0 { "Aa" } else { "BB" });
        }
        s.push_str("\":1");
    }
    s.push('}');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_and_magnitude() {
        assert_eq!(
            parse_spec("nested-json:42").unwrap(),
            PayloadSpec {
                name: "nested-json".into(),
                magnitude: 42
            }
        );
        // Omitted magnitude uses the kind default.
        assert_eq!(parse_spec("nested-json").unwrap().magnitude, 10_000);
    }

    #[test]
    fn rejects_unknown_bad_and_oversized() {
        assert!(matches!(
            parse_spec("nope:1"),
            Err(PayloadError::Unknown(_))
        ));
        assert!(matches!(
            parse_spec("nested-json:x"),
            Err(PayloadError::BadMagnitude(_))
        ));
        assert!(matches!(
            parse_spec("billion-laughs:9999"),
            Err(PayloadError::TooLarge { max: 12, .. })
        ));
    }

    #[test]
    fn nested_json_is_balanced_and_correct() {
        let out = String::from_utf8(generate_str("nested-json:3").unwrap()).unwrap();
        assert_eq!(out, "{\"a\":{\"a\":{\"a\":1}}}");
        assert_eq!(out.matches('{').count(), 3);
        assert_eq!(out.matches('}').count(), 3);
    }

    #[test]
    fn markdown_blockquote_is_one_line_of_markers() {
        let out = String::from_utf8(generate_str("nested-markdown-blockquote:5").unwrap()).unwrap();
        assert_eq!(out, ">>>>> x");
    }

    #[test]
    fn json_array_and_keys_are_valid_shapes() {
        assert_eq!(
            String::from_utf8(generate_str("json-array:3").unwrap()).unwrap(),
            "[0,1,2]"
        );
        assert_eq!(
            String::from_utf8(generate_str("json-object-keys:2").unwrap()).unwrap(),
            "{\"k0\":0,\"k1\":1}"
        );
    }

    #[test]
    fn hash_collision_keys_all_collide_under_java_hash() {
        // Java String.hashCode: h = 31*h + c.
        fn jhash(s: &str) -> i32 {
            s.bytes()
                .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
        }
        let out = String::from_utf8(generate_str("hash-collision:8").unwrap()).unwrap();
        // Pull the keys out of the JSON object.
        let keys: Vec<&str> = out
            .split('"')
            .enumerate()
            .filter_map(|(i, part)| (i % 2 == 1).then_some(part))
            .collect();
        assert_eq!(keys.len(), 8);
        let first = jhash(keys[0]);
        for k in &keys {
            assert_eq!(jhash(k), first, "key {k} should collide");
        }
    }

    #[test]
    fn billion_laughs_is_small_source_with_expected_entity_count() {
        let out = String::from_utf8(generate_str("billion-laughs:5").unwrap()).unwrap();
        assert!(out.contains("<!ENTITY lol4"));
        assert!(out.contains("&lol4;")); // top-level reference
        assert!(
            out.len() < 2000,
            "source stays tiny; expansion is the parser's problem"
        );
    }

    #[test]
    fn zalgo_has_the_requested_combining_marks() {
        let out = String::from_utf8(generate_str("zalgo:6").unwrap()).unwrap();
        assert_eq!(out.chars().filter(|&c| c == '\u{0301}').count(), 6);
    }

    #[test]
    fn every_catalog_entry_generates_at_its_default() {
        for p in CATALOG {
            let bytes = generate_str(p.name).expect(p.name);
            assert!(!bytes.is_empty(), "{} produced no bytes", p.name);
        }
    }
}
