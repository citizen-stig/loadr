//! WASM extractor plugin: decode a raw protobuf message body and extract a
//! single field by number (or a dotted path into nested messages), rendering
//! it as a string according to a declared `kind`.
//!
//! Protobuf wire parsing is hand-rolled (varint + tag/wire-type) so the crate
//! compiles cleanly to `wasm32-wasip2` with no `prost`/`protoc` dependency.
//!
//! Config: `{"field": 2, "kind": "string"}`
//!   - `field`: a field number (JSON number, e.g. `2`) or a dotted path into
//!     nested messages (JSON string, e.g. `"3.1"` = field 1 inside field 3).
//!   - `kind`: how to render the extracted wire value. One of:
//!     `string`, `bytes`/`base64`, `hex`, `int`/`int32`/`int64`/`enum`,
//!     `uint`/`uint32`/`uint64`, `bool`, `sint`/`sint32`/`sint64`,
//!     `double`, `float`, `fixed64`/`sfixed64`, `fixed32`/`sfixed32`.

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-plugin",
});

use exports::loadr::plugin::extractor::Guest as Extractor;
use exports::loadr::plugin::meta::{Guest as Meta, Info};

// ---------------------------------------------------------------------------
// Pure, host-testable protobuf decode logic (no WIT / no wasm types below).
// ---------------------------------------------------------------------------

/// A decoded protobuf wire value.
#[derive(Debug, Clone, PartialEq)]
enum WireValue {
    /// Wire type 0: base-128 varint (int*, uint*, sint*, bool, enum).
    Varint(u64),
    /// Wire type 1: fixed 64-bit little-endian (fixed64, sfixed64, double).
    Fixed64(u64),
    /// Wire type 2: length-delimited (string, bytes, embedded message).
    Len(Vec<u8>),
    /// Wire type 5: fixed 32-bit little-endian (fixed32, sfixed32, float).
    Fixed32(u32),
}

/// Read a base-128 varint starting at `pos`. Returns `(value, next_pos)`.
fn read_varint(data: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut i = pos;
    loop {
        if i >= data.len() {
            return None;
        }
        // A protobuf varint is at most 10 bytes; reject overflow.
        if shift >= 64 {
            return None;
        }
        let b = data[i];
        result |= ((b & 0x7f) as u64) << shift;
        i += 1;
        if b & 0x80 == 0 {
            return Some((result, i));
        }
        shift += 7;
    }
}

/// Scan a single protobuf message, returning the value of `field`. When the
/// field appears more than once, the last occurrence wins (proto merge
/// semantics for singular fields). Returns `None` when absent or malformed.
fn field_in_message(msg: &[u8], field: u32) -> Option<WireValue> {
    let mut pos = 0usize;
    let mut found: Option<WireValue> = None;
    while pos < msg.len() {
        let (tag, np) = read_varint(msg, pos)?;
        pos = np;
        let fnum = (tag >> 3) as u32;
        let wtype = (tag & 0x07) as u8;
        let val = match wtype {
            0 => {
                let (v, np) = read_varint(msg, pos)?;
                pos = np;
                WireValue::Varint(v)
            }
            1 => {
                if pos + 8 > msg.len() {
                    return None;
                }
                let mut b = [0u8; 8];
                b.copy_from_slice(&msg[pos..pos + 8]);
                pos += 8;
                WireValue::Fixed64(u64::from_le_bytes(b))
            }
            2 => {
                let (len, np) = read_varint(msg, pos)?;
                pos = np;
                let len = len as usize;
                if pos + len > msg.len() {
                    return None;
                }
                let v = msg[pos..pos + len].to_vec();
                pos += len;
                WireValue::Len(v)
            }
            5 => {
                if pos + 4 > msg.len() {
                    return None;
                }
                let mut b = [0u8; 4];
                b.copy_from_slice(&msg[pos..pos + 4]);
                pos += 4;
                WireValue::Fixed32(u32::from_le_bytes(b))
            }
            // Wire types 3/4 (deprecated groups) are unsupported.
            _ => return None,
        };
        if fnum == field {
            found = Some(val);
        }
    }
    found
}

/// Walk a dotted field path into (possibly nested) messages and return the
/// wire value of the final element.
fn extract_path(body: &[u8], path: &[u32]) -> Option<WireValue> {
    if path.is_empty() {
        return None;
    }
    let mut current: Vec<u8> = body.to_vec();
    for (i, &f) in path.iter().enumerate() {
        let v = field_in_message(&current, f)?;
        if i == path.len() - 1 {
            return Some(v);
        }
        // Intermediate path elements must be embedded messages to descend into.
        match v {
            WireValue::Len(bytes) => current = bytes,
            _ => return None,
        }
    }
    None
}

/// Parse a dotted field spec such as `"3.1"` (or a bare `"2"`) into a path of
/// field numbers. Rejects empty / non-numeric / zero components (protobuf
/// field numbers start at 1).
fn parse_path(spec: &str) -> Option<Vec<u32>> {
    let spec = spec.trim();
    if spec.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in spec.split('.') {
        let n: u32 = part.trim().parse().ok()?;
        if n == 0 {
            return None;
        }
        out.push(n);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// ZigZag-decode a varint into its signed representation.
fn zigzag_decode(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// Lowercase hex encoding of a byte slice.
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// Standard base64 (with padding) encoding of a byte slice.
fn to_base64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Render a decoded wire value as a string according to `kind`. Returns `None`
/// when the wire type does not match the requested kind.
fn render(v: &WireValue, kind: &str) -> Option<String> {
    match kind {
        "string" | "str" => match v {
            WireValue::Len(b) => String::from_utf8(b.clone()).ok(),
            _ => None,
        },
        "bytes" | "base64" => match v {
            WireValue::Len(b) => Some(to_base64(b)),
            _ => None,
        },
        "hex" => match v {
            WireValue::Len(b) => Some(to_hex(b)),
            _ => None,
        },
        "message" => match v {
            // Embedded messages are opaque here; hand back hex of the raw bytes.
            WireValue::Len(b) => Some(to_hex(b)),
            _ => None,
        },
        "int" | "int32" | "int64" | "enum" => match v {
            WireValue::Varint(x) => Some((*x as i64).to_string()),
            _ => None,
        },
        "uint" | "uint32" | "uint64" => match v {
            WireValue::Varint(x) => Some(x.to_string()),
            _ => None,
        },
        "bool" => match v {
            WireValue::Varint(x) => Some((*x != 0).to_string()),
            _ => None,
        },
        "sint" | "sint32" | "sint64" => match v {
            WireValue::Varint(x) => Some(zigzag_decode(*x).to_string()),
            _ => None,
        },
        "double" => match v {
            WireValue::Fixed64(x) => Some(f64::from_bits(*x).to_string()),
            _ => None,
        },
        "fixed64" => match v {
            WireValue::Fixed64(x) => Some(x.to_string()),
            _ => None,
        },
        "sfixed64" => match v {
            WireValue::Fixed64(x) => Some((*x as i64).to_string()),
            _ => None,
        },
        "float" => match v {
            WireValue::Fixed32(x) => Some(f32::from_bits(*x).to_string()),
            _ => None,
        },
        "fixed32" => match v {
            WireValue::Fixed32(x) => Some(x.to_string()),
            _ => None,
        },
        "sfixed32" => match v {
            WireValue::Fixed32(x) => Some((*x as i32).to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// End-to-end decode: locate `spec` in `body` and render it as `kind`.
fn decode(body: &[u8], spec: &str, kind: &str) -> Option<String> {
    let path = parse_path(spec)?;
    let value = extract_path(body, &path)?;
    render(&value, kind)
}

// ---------------------------------------------------------------------------
// WIT plugin surface.
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct Config {
    field: serde_json::Value,
    #[serde(default = "default_kind")]
    kind: String,
}

fn default_kind() -> String {
    "string".to_string()
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "protobuf-decode".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "extractor".to_string(),
            description:
                "Decodes a protobuf message body and extracts a field by number or dotted path"
                    .to_string(),
        }
    }
}

impl Extractor for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let cfg: Config = serde_json::from_str(&config).ok()?;
        // `field` may be a bare number (2) or a dotted path string ("3.1").
        let spec = match &cfg.field {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => return None,
        };
        decode(&body, &spec, &cfg.kind)
    }
}

export!(Plugin);

// ---------------------------------------------------------------------------
// Host-side unit tests for the pure decode logic.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Canonical protobuf example message:
    //   field 1 (varint)  = 150
    //   field 2 (string)  = "testing"
    fn canonical() -> Vec<u8> {
        vec![
            0x08, 0x96, 0x01, // field 1 varint 150
            0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g', // field 2 "testing"
        ]
    }

    #[test]
    fn read_varint_single_byte() {
        assert_eq!(read_varint(&[0x2a], 0), Some((42, 1)));
    }

    #[test]
    fn read_varint_multi_byte() {
        // 150 = 0x96 0x01
        assert_eq!(read_varint(&[0x96, 0x01], 0), Some((150, 2)));
    }

    #[test]
    fn read_varint_respects_offset() {
        assert_eq!(read_varint(&[0xff, 0x96, 0x01], 1), Some((150, 3)));
    }

    #[test]
    fn read_varint_truncated_is_none() {
        // Continuation bit set but no following byte.
        assert_eq!(read_varint(&[0x96], 0), None);
    }

    #[test]
    fn read_varint_overflow_is_none() {
        // 11 bytes with continuation bits => overflow, must reject.
        let data = [0xff; 11];
        assert_eq!(read_varint(&data, 0), None);
    }

    #[test]
    fn field_varint_lookup() {
        let msg = canonical();
        assert_eq!(field_in_message(&msg, 1), Some(WireValue::Varint(150)));
    }

    #[test]
    fn field_string_lookup() {
        let msg = canonical();
        assert_eq!(
            field_in_message(&msg, 2),
            Some(WireValue::Len(b"testing".to_vec()))
        );
    }

    #[test]
    fn field_absent_is_none() {
        assert_eq!(field_in_message(&canonical(), 9), None);
    }

    #[test]
    fn field_last_occurrence_wins() {
        // field 1 = 1, then field 1 = 2 => expect 2
        let msg = vec![0x08, 0x01, 0x08, 0x02];
        assert_eq!(field_in_message(&msg, 1), Some(WireValue::Varint(2)));
    }

    #[test]
    fn decode_string_by_number() {
        assert_eq!(
            decode(&canonical(), "2", "string"),
            Some("testing".to_string())
        );
    }

    #[test]
    fn decode_int_by_number() {
        assert_eq!(decode(&canonical(), "1", "int"), Some("150".to_string()));
    }

    #[test]
    fn decode_uint() {
        assert_eq!(decode(&canonical(), "1", "uint"), Some("150".to_string()));
    }

    #[test]
    fn decode_nested_path() {
        // outer field 3 = message { field 1 (varint) = 42 }
        //   inner: [0x08, 0x2a]
        //   outer: tag=(3<<3)|2=0x1a, len=2
        let msg = vec![0x1a, 0x02, 0x08, 0x2a];
        assert_eq!(decode(&msg, "3.1", "int"), Some("42".to_string()));
    }

    #[test]
    fn decode_nested_string() {
        // outer field 4 = message { field 2 (string) = "hi" }
        //   inner: tag 0x12, len 2, "hi"
        //   outer: tag=(4<<3)|2=0x22, len=4
        let msg = vec![0x22, 0x04, 0x12, 0x02, b'h', b'i'];
        assert_eq!(decode(&msg, "4.2", "string"), Some("hi".to_string()));
    }

    #[test]
    fn decode_sint_zigzag() {
        // sint field 4 = -1 => zigzag(1). tag=(4<<3)|0=0x20, value 0x01
        let msg = vec![0x20, 0x01];
        assert_eq!(decode(&msg, "4", "sint"), Some("-1".to_string()));
    }

    #[test]
    fn decode_bool() {
        let msg = vec![0x08, 0x01];
        assert_eq!(decode(&msg, "1", "bool"), Some("true".to_string()));
        let msg0 = vec![0x08, 0x00];
        assert_eq!(decode(&msg0, "1", "bool"), Some("false".to_string()));
    }

    #[test]
    fn decode_double() {
        // field 5 fixed64 = 1.0 => bits 0x3FF0000000000000
        // tag=(5<<3)|1=0x29, then 8 LE bytes
        let mut msg = vec![0x29];
        msg.extend_from_slice(&1.0f64.to_le_bytes());
        assert_eq!(decode(&msg, "5", "double"), Some("1".to_string()));
    }

    #[test]
    fn decode_float() {
        // field 6 fixed32 = 2.5 => tag=(6<<3)|5=0x35
        let mut msg = vec![0x35];
        msg.extend_from_slice(&2.5f32.to_le_bytes());
        assert_eq!(decode(&msg, "6", "float"), Some("2.5".to_string()));
    }

    #[test]
    fn decode_fixed64() {
        let mut msg = vec![0x29];
        msg.extend_from_slice(&123u64.to_le_bytes());
        assert_eq!(decode(&msg, "5", "fixed64"), Some("123".to_string()));
    }

    #[test]
    fn decode_bytes_base64() {
        // field 2 (bytes) = [0xde, 0xad, 0xbe, 0xef]
        let msg = vec![0x12, 0x04, 0xde, 0xad, 0xbe, 0xef];
        assert_eq!(decode(&msg, "2", "base64"), Some("3q2+7w==".to_string()));
    }

    #[test]
    fn decode_bytes_hex() {
        let msg = vec![0x12, 0x04, 0xde, 0xad, 0xbe, 0xef];
        assert_eq!(decode(&msg, "2", "hex"), Some("deadbeef".to_string()));
    }

    #[test]
    fn kind_mismatch_is_none() {
        // field 1 is a varint; asking for a string must fail cleanly.
        assert_eq!(decode(&canonical(), "1", "string"), None);
    }

    #[test]
    fn unknown_kind_is_none() {
        assert_eq!(decode(&canonical(), "2", "nonsense"), None);
    }

    #[test]
    fn parse_path_variants() {
        assert_eq!(parse_path("2"), Some(vec![2]));
        assert_eq!(parse_path("3.1.4"), Some(vec![3, 1, 4]));
        assert_eq!(parse_path(" 3 . 1 "), Some(vec![3, 1]));
        assert_eq!(parse_path(""), None);
        assert_eq!(parse_path("0"), None);
        assert_eq!(parse_path("1.x"), None);
        assert_eq!(parse_path("1..2"), None);
    }

    #[test]
    fn zigzag_values() {
        assert_eq!(zigzag_decode(0), 0);
        assert_eq!(zigzag_decode(1), -1);
        assert_eq!(zigzag_decode(2), 1);
        assert_eq!(zigzag_decode(3), -2);
        assert_eq!(zigzag_decode(4294967294), 2147483647);
    }

    #[test]
    fn descend_into_non_message_is_none() {
        // field 1 is a varint, not a message; path "1.2" cannot descend.
        assert_eq!(decode(&canonical(), "1.2", "int"), None);
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    }
}
