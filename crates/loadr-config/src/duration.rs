//! Human-friendly duration type used throughout the YAML schema.
//!
//! Accepts strings such as `300ms`, `2s`, `1m30s`, `1h` (via [`humantime`]) and
//! bare numbers, which are interpreted as seconds.

use std::fmt;
use std::time::Duration;

use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// A duration that serializes to/from human-readable strings (`1m30s`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Dur(pub Duration);

impl Dur {
    pub const ZERO: Dur = Dur(Duration::ZERO);

    pub fn from_secs(secs: u64) -> Self {
        Dur(Duration::from_secs(secs))
    }

    pub fn from_millis(ms: u64) -> Self {
        Dur(Duration::from_millis(ms))
    }

    pub fn as_duration(&self) -> Duration {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// Parse a duration from a string such as `300ms`, `2s` or `1m30s`.
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        // Allow bare numbers (seconds), including fractional ones.
        if let Ok(secs) = trimmed.parse::<f64>() {
            if secs < 0.0 || !secs.is_finite() {
                return Err(format!("duration must be a non-negative number, got `{s}`"));
            }
            return Ok(Dur(Duration::from_secs_f64(secs)));
        }
        humantime::parse_duration(trimmed)
            .map(Dur)
            .map_err(|e| format!("invalid duration `{s}`: {e} (try e.g. `500ms`, `30s`, `1m30s`)"))
    }
}

impl From<Duration> for Dur {
    fn from(d: Duration) -> Self {
        Dur(d)
    }
}

impl From<Dur> for Duration {
    fn from(d: Dur) -> Self {
        d.0
    }
}

impl fmt::Display for Dur {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", humantime::format_duration(self.0))
    }
}

impl Serialize for Dur {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

struct DurVisitor;

impl Visitor<'_> for DurVisitor {
    type Value = Dur;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a duration string like `30s`, `1m30s` or a number of seconds"
        )
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Dur, E> {
        Dur::parse(v).map_err(de::Error::custom)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Dur, E> {
        Ok(Dur(Duration::from_secs(v)))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Dur, E> {
        if v < 0 {
            return Err(de::Error::custom("duration cannot be negative"));
        }
        Ok(Dur(Duration::from_secs(v as u64)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Dur, E> {
        if v < 0.0 || !v.is_finite() {
            return Err(de::Error::custom("duration must be a non-negative number"));
        }
        Ok(Dur(Duration::from_secs_f64(v)))
    }
}

impl<'de> Deserialize<'de> for Dur {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DurVisitor)
    }
}

impl JsonSchema for Dur {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Duration".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "title": "Duration",
            "description": "A duration: a string like `300ms`, `30s`, `1m30s`, `1h`, or a number of seconds",
            "anyOf": [
                { "type": "string", "pattern": "^\\s*([0-9]+(\\.[0-9]+)?\\s*(ns|us|µs|ms|s|m|h|d|w)?\\s*)+$" },
                { "type": "number", "minimum": 0.0 }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_humantime_strings() {
        assert_eq!(Dur::parse("300ms").unwrap(), Dur::from_millis(300));
        assert_eq!(Dur::parse("2s").unwrap(), Dur::from_secs(2));
        assert_eq!(Dur::parse("1m30s").unwrap(), Dur::from_secs(90));
        assert_eq!(Dur::parse("1h").unwrap(), Dur::from_secs(3600));
    }

    #[test]
    fn parses_bare_numbers_as_seconds() {
        assert_eq!(Dur::parse("5").unwrap(), Dur::from_secs(5));
        assert_eq!(Dur::parse("0.5").unwrap(), Dur::from_millis(500));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Dur::parse("five seconds-ish").is_err());
        assert!(Dur::parse("-3s").is_err());
    }

    #[test]
    fn yaml_round_trip() {
        let d: Dur = serde_yaml::from_str("1m30s").unwrap();
        assert_eq!(d, Dur::from_secs(90));
        let d: Dur = serde_yaml::from_str("45").unwrap();
        assert_eq!(d, Dur::from_secs(45));
        let s = serde_yaml::to_string(&Dur::from_secs(90)).unwrap();
        assert_eq!(s.trim(), "1m 30s");
        let back: Dur = serde_yaml::from_str(&s).unwrap();
        assert_eq!(back, Dur::from_secs(90));
    }
}
