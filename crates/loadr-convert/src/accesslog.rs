//! Access-log importer: reconstruct a load profile from nginx/apache logs.
//!
//! Parses COMBINED-format lines (`$remote_addr - $remote_user [$time_local]
//! "METHOD /path HTTP/x" status bytes "referer" "user-agent"`), tolerating the
//! plain COMMON format (no referer/user-agent) and custom formats with extra
//! trailing fields. From the timestamps it derives the observed duration and
//! the average/peak arrival rates, groups requests by method + normalised path
//! (numeric ids, UUIDs and long hex segments become `${vars.id}`), and emits
//! ONE scenario: a `constant-arrival-rate` executor at the observed average
//! rate whose flow is a single weighted `random` block over the top-N
//! endpoints — the same shape as `examples/40-scenario-weights.yaml`.
//!
//! Everything is best-effort; anything approximated becomes a
//! [`ConversionWarning`]. The produced plan passes `loadr_config::validate`.

use std::collections::HashMap;

use indexmap::IndexMap;
use regex::Regex;

use loadr_config::{
    Dur, ExecutorKind, RandomChoice, RandomStep, RequestStep, Scenario, Step, SwitchStrategy,
    TestPlan,
};

use crate::{Conversion, ConversionWarning, ConvertError};

/// How many distinct endpoints to keep in the weighted mix.
const DEFAULT_TOP_ENDPOINTS: usize = 20;

/// Month abbreviations as produced by the CLF `%d/%b/%Y` time format.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// One successfully parsed log line.
#[derive(Debug)]
struct ParsedLine {
    method: String,
    path: String,
    /// Unix timestamp (seconds, UTC).
    epoch: i64,
    /// The line had content beyond the standard COMBINED fields.
    extra_fields: bool,
}

/// Compiled patterns shared across all lines (same idea as `har.rs`'s
/// `Patterns`, specialised to path segments).
struct LogPatterns {
    line: Regex,
    numeric: Regex,
    uuid: Regex,
    hex: Regex,
}

impl LogPatterns {
    fn new() -> LogPatterns {
        LogPatterns {
            // remote ident user [time] "request" status bytes ["referer" "ua"] extra
            line: Regex::new(
                r#"^(\S+)\s+(\S+)\s+(\S+)\s+\[([^\]]+)\]\s+"([^"]*)"\s+(\d{3})\s+(\d+|-)(?:\s+"([^"]*)"\s+"([^"]*)")?(.*)$"#,
            )
            .unwrap(),
            numeric: Regex::new(r"^\d+$").unwrap(),
            uuid: Regex::new(
                r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$",
            )
            .unwrap(),
            hex: Regex::new(r"^[0-9a-fA-F]{16,}$").unwrap(),
        }
    }

    /// True for a path segment that looks like a per-record identifier.
    fn segment_is_id(&self, seg: &str) -> bool {
        self.numeric.is_match(seg) || self.uuid.is_match(seg) || self.hex.is_match(seg)
    }
}

/// Days from 1970-01-01 for a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let mp = i64::from((m + 9) % 12); // Mar=0 .. Feb=11
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `+0100` / `-0700` → offset from UTC in seconds.
fn tz_offset_seconds(tz: &str) -> Option<i64> {
    let (sign, digits) = match tz.as_bytes().first()? {
        b'+' => (1, &tz[1..]),
        b'-' => (-1, &tz[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: i64 = digits[..2].parse().ok()?;
    let m: i64 = digits[2..].parse().ok()?;
    Some(sign * (h * 3_600 + m * 60))
}

/// Parse a CLF timestamp (`10/Oct/2000:13:55:36 -0700`) to Unix seconds.
/// A missing timezone is treated as UTC.
fn parse_clf_timestamp(s: &str) -> Option<i64> {
    let mut halves = s.splitn(2, ' ');
    let dt = halves.next()?;
    let tz = halves.next().unwrap_or("+0000");
    let mut date = dt.splitn(3, '/');
    let day: u32 = date.next()?.parse().ok()?;
    let mon = date.next()?;
    let rest = date.next()?; // "2000:13:55:36"
    let month = MONTHS.iter().position(|m| m.eq_ignore_ascii_case(mon))? as u32 + 1;
    let mut time = rest.splitn(4, ':');
    let year: i64 = time.next()?.parse().ok()?;
    let hour: i64 = time.next()?.parse().ok()?;
    let min: i64 = time.next()?.parse().ok()?;
    let sec: i64 = time.next()?.parse().ok()?;
    if day == 0 || day > 31 || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + min * 60 + sec;
    Some(local - tz_offset_seconds(tz)?)
}

/// Parse one line; `None` means malformed (bad shape, request or timestamp).
fn parse_line(pats: &LogPatterns, line: &str) -> Option<ParsedLine> {
    let caps = pats.line.captures(line)?;
    let epoch = parse_clf_timestamp(caps.get(4)?.as_str())?;
    let mut request = caps.get(5)?.as_str().split_whitespace();
    let method = request.next()?;
    let target = request.next()?;
    if !method.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    // Absolute-form targets (forward-proxy logs) keep only path-and-query.
    let path = match target.find("://") {
        Some(i) => {
            let after = &target[i + 3..];
            match after.find('/') {
                Some(j) => &after[j..],
                None => "/",
            }
        }
        None if target.starts_with('/') || target == "*" => target,
        None => return None,
    };
    let extra_fields = !caps.get(10).map_or("", |m| m.as_str()).trim().is_empty();
    Some(ParsedLine {
        method: method.to_ascii_uppercase(),
        path: path.to_string(),
        epoch,
        extra_fields,
    })
}

/// Replace identifier-looking path segments with `${vars.id}` and drop the
/// query string. Returns the normalised path and how many segments changed.
fn normalize_path(pats: &LogPatterns, path: &str) -> (String, usize) {
    let no_query = path.split(['?', '#']).next().unwrap_or(path);
    let mut changed = 0usize;
    let segments: Vec<String> = no_query
        .split('/')
        .map(|seg| {
            if !seg.is_empty() && pats.segment_is_id(seg) {
                changed += 1;
                "${vars.id}".to_string()
            } else {
                seg.to_string()
            }
        })
        .collect();
    (segments.join("/"), changed)
}

/// Convert an nginx/apache COMBINED (or COMMON) access log into a loadr
/// [`TestPlan`] that replays the observed traffic shape.
pub fn convert_accesslog(source: &str) -> Result<Conversion, ConvertError> {
    convert_with_top_n(source, DEFAULT_TOP_ENDPOINTS)
}

/// Seam for tests: [`convert_accesslog`] with a configurable endpoint cap.
fn convert_with_top_n(source: &str, top_n: usize) -> Result<Conversion, ConvertError> {
    let pats = LogPatterns::new();
    let mut malformed = 0usize;
    let mut extra_lines = 0usize;
    let mut normalised_segments = 0usize;
    let mut queries_dropped = 0usize;
    let mut buckets: HashMap<i64, u64> = HashMap::new();
    let mut endpoints: IndexMap<(String, String), u64> = IndexMap::new();
    let mut total = 0u64;

    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let parsed = match parse_line(&pats, line) {
            Some(p) => p,
            None => {
                malformed += 1;
                continue;
            }
        };
        if parsed.extra_fields {
            extra_lines += 1;
        }
        if parsed.path.contains('?') {
            queries_dropped += 1;
        }
        let (path, changed) = normalize_path(&pats, &parsed.path);
        normalised_segments += changed;
        *buckets.entry(parsed.epoch).or_insert(0) += 1;
        *endpoints.entry((parsed.method, path)).or_insert(0) += 1;
        total += 1;
    }

    if total == 0 {
        return Err(ConvertError::AccessLog(format!(
            "no parseable request lines found ({malformed} malformed line(s))"
        )));
    }

    // ---- traffic shape ----------------------------------------------------
    // Bucket counts per second give duration + average and peak arrival rates.
    let first = *buckets.keys().min().unwrap();
    let last = *buckets.keys().max().unwrap();
    let duration_secs = (last - first + 1) as u64;
    let peak = *buckets.values().max().unwrap();
    let avg = total as f64 / duration_secs as f64;
    let rate = (avg.max(0.01) * 100.0).round() / 100.0;

    // ---- weighted endpoint mix ---------------------------------------------
    // Stable sort: ties keep first-seen (IndexMap insertion) order.
    let mut ranked: Vec<((String, String), u64)> = endpoints.into_iter().collect();
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    let total_endpoints = ranked.len();
    let kept: Vec<((String, String), u64)> = ranked.into_iter().take(top_n.max(1)).collect();
    let kept_requests: u64 = kept.iter().map(|(_, c)| c).sum();

    let mut bodied_endpoints = 0usize;
    let mut choices = Vec::with_capacity(kept.len());
    for ((method, path), count) in &kept {
        if matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
            bodied_endpoints += 1;
        }
        let name = format!("{method} {path}");
        choices.push(RandomChoice {
            weight: Some(*count as f64),
            name: Some(name.clone()),
            steps: vec![Step::Request(Box::new(RequestStep {
                name: Some(name),
                method: Some(method.clone()),
                url: path.clone(),
                ..Default::default()
            }))],
        });
    }

    let scenario = Scenario {
        executor: ExecutorKind::ConstantArrivalRate,
        rate: Some(rate),
        duration: Some(Dur::from_secs(duration_secs)),
        // Sized for the observed per-second peak (assuming ~1s iterations).
        pre_allocated_vus: Some(peak.max(1)),
        max_vus: Some((peak * 2).max(2)),
        flow: vec![Step::Random(RandomStep {
            strategy: SwitchStrategy::Weighted,
            choices,
        })],
        ..Default::default()
    };
    let mut scenarios = IndexMap::new();
    scenarios.insert("replayed_traffic".to_string(), scenario);

    // Placeholder so `${vars.id}` resolves; the warning below tells the user
    // to swap it for a `data:` feeder of real identifiers.
    let mut variables = IndexMap::new();
    if normalised_segments > 0 {
        variables.insert("id".to_string(), serde_json::Value::String("1".into()));
    }

    let plan = TestPlan {
        name: Some("access log replay".to_string()),
        description: Some(format!(
            "Imported from an access log by `loadr convert`: {total} requests over \
             {duration_secs}s (avg {avg:.2} req/s, peak {peak} req/s)."
        )),
        variables,
        scenarios,
        ..Default::default()
    };

    // ---- warnings -----------------------------------------------------------
    let mut warnings = Vec::new();
    if malformed > 0 {
        warnings.push(ConversionWarning {
            element: "parser".to_string(),
            message: format!("skipped {malformed} malformed line(s)"),
        });
    }
    if extra_lines > 0 {
        warnings.push(ConversionWarning {
            element: "parser".to_string(),
            message: format!(
                "{extra_lines} line(s) had extra fields after the standard COMBINED fields \
                 (custom log_format?); the extras were ignored"
            ),
        });
    }
    warnings.push(ConversionWarning {
        element: "scenario `replayed_traffic`".to_string(),
        message: format!(
            "constant-arrival-rate set to the observed average ({rate} req/s); the observed \
             peak was {peak} req/s — consider ramping-arrival-rate up to the peak for \
             worst-case load"
        ),
    });
    warnings.push(ConversionWarning {
        element: "defaults.http.base_url".to_string(),
        message:
            "access logs do not record scheme or host — set `defaults.http.base_url` before running"
                .to_string(),
    });
    if queries_dropped > 0 {
        warnings.push(ConversionWarning {
            element: "paths".to_string(),
            message: format!(
                "dropped query strings on {queries_dropped} request(s) when grouping — \
                 re-add important parameters via `params:`"
            ),
        });
    }
    if normalised_segments > 0 {
        warnings.push(ConversionWarning {
            element: "paths".to_string(),
            message: format!(
                "normalised {normalised_segments} id-like path segment(s) (numbers, UUIDs, \
                 hashes) to `${{vars.id}}` — replace the placeholder variable with a `data:` \
                 feeder of real ids"
            ),
        });
    }
    if total_endpoints > kept.len() {
        warnings.push(ConversionWarning {
            element: "endpoints".to_string(),
            message: format!(
                "kept the top {} of {total_endpoints} endpoints ({:.1}% of requests); the \
                 long tail was dropped",
                kept.len(),
                100.0 * kept_requests as f64 / total as f64
            ),
        });
    }
    if bodied_endpoints > 0 {
        warnings.push(ConversionWarning {
            element: "bodies".to_string(),
            message: format!(
                "{bodied_endpoints} endpoint(s) use POST/PUT/PATCH; access logs do not \
                 record request bodies — add realistic `body:` payloads"
            ),
        });
    }

    Ok(Conversion { plan, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a COMBINED-format line at second `sec` of a fixed minute.
    fn line(sec: u64, method: &str, path: &str) -> String {
        format!(
            "203.0.113.7 - alice [10/Oct/2000:13:55:{sec:02} +0000] \
             \"{method} {path} HTTP/1.1\" 200 512 \"-\" \"curl/8.0\""
        )
    }

    fn random_step(c: &Conversion) -> &RandomStep {
        let scenario = c.plan.scenarios.get("replayed_traffic").unwrap();
        match &scenario.flow[0] {
            Step::Random(r) => r,
            other => panic!("expected random step, got {other:?}"),
        }
    }

    #[test]
    fn parses_clf_timestamps() {
        assert_eq!(parse_clf_timestamp("01/Jan/1970:00:00:00 +0000"), Some(0));
        // Offsets shift back to UTC.
        assert_eq!(parse_clf_timestamp("01/Jan/1970:01:00:00 +0100"), Some(0));
        assert_eq!(
            parse_clf_timestamp("10/Oct/2000:13:55:36 -0700"),
            Some(971_211_336)
        );
        // A missing zone is treated as UTC.
        assert_eq!(
            parse_clf_timestamp("10/Oct/2000:13:55:36"),
            Some(971_186_136)
        );
        assert_eq!(parse_clf_timestamp("10/Foo/2000:13:55:36 +0000"), None);
        assert_eq!(parse_clf_timestamp("10/Oct/2000:13:55:36 GMT"), None);
    }

    #[test]
    fn tolerates_combined_common_ipv6_and_extra_fields() {
        let log = [
            // Full COMBINED format.
            r#"203.0.113.7 - alice [10/Oct/2000:13:55:36 +0000] "GET /a HTTP/1.1" 200 512 "https://ref.example" "Mozilla/5.0""#,
            // COMMON format: no referer/user-agent.
            r#"198.51.100.2 - - [10/Oct/2000:13:55:36 +0000] "GET /a HTTP/1.0" 200 100"#,
            // IPv6 remote address.
            r#"2001:db8::1 - - [10/Oct/2000:13:55:37 +0000] "GET /a HTTP/1.1" 200 1 "-" "curl/8""#,
            // Extra trailing fields from a custom log_format.
            r#"203.0.113.7 - - [10/Oct/2000:13:55:37 +0000] "GET /a HTTP/1.1" 200 1 "-" "curl/8" 0.042 backend=api-1"#,
            // Malformed noise.
            "garbage line",
        ]
        .join("\n");
        let c = convert_accesslog(&log).expect("convert");
        let r = random_step(&c);
        assert_eq!(r.choices.len(), 1, "all four lines hit the same endpoint");
        assert_eq!(r.choices[0].weight, Some(4.0));
        assert!(c
            .warnings
            .iter()
            .any(|w| w.message.contains("skipped 1 malformed")));
        assert!(c
            .warnings
            .iter()
            .any(|w| w.message.contains("extras were ignored")));
    }

    #[test]
    fn normalises_id_like_path_segments() {
        let pats = LogPatterns::new();
        assert_eq!(
            normalize_path(&pats, "/users/123/orders/456").0,
            "/users/${vars.id}/orders/${vars.id}"
        );
        assert_eq!(
            normalize_path(&pats, "/items/3f2504e0-4f89-41d3-9a0c-0305e82c3301").0,
            "/items/${vars.id}"
        );
        assert_eq!(
            normalize_path(&pats, "/blobs/deadbeefdeadbeef01").0,
            "/blobs/${vars.id}"
        );
        // Query strings are dropped; version-ish segments survive.
        assert_eq!(
            normalize_path(&pats, "/api/v1/users?page=2").0,
            "/api/v1/users"
        );

        // Through the converter: placeholder variable + data-feeder note.
        let log = line(1, "GET", "/users/123");
        let c = convert_accesslog(&log).expect("convert");
        assert!(c.plan.variables.contains_key("id"));
        let r = random_step(&c);
        assert_eq!(r.choices[0].name.as_deref(), Some("GET /users/${vars.id}"));
        assert!(c.warnings.iter().any(|w| w.message.contains("feeder")));
    }

    #[test]
    fn derives_duration_average_and_peak_rates() {
        // 10 requests over the 5-second window 30..=34, 4 of them in second 32.
        let secs = [30, 31, 32, 32, 32, 32, 33, 33, 34, 34];
        let log = secs
            .iter()
            .map(|s| line(*s, "GET", "/a"))
            .collect::<Vec<_>>()
            .join("\n");
        let c = convert_accesslog(&log).expect("convert");
        let scenario = c.plan.scenarios.get("replayed_traffic").unwrap();
        assert_eq!(scenario.executor, ExecutorKind::ConstantArrivalRate);
        assert_eq!(scenario.rate, Some(2.0), "10 requests / 5s");
        assert_eq!(scenario.duration, Some(Dur::from_secs(5)));
        assert_eq!(scenario.pre_allocated_vus, Some(4), "sized for the peak");
        assert!(c
            .warnings
            .iter()
            .any(|w| w.message.contains("peak was 4 req/s")));
    }

    #[test]
    fn weights_endpoints_by_frequency() {
        let log = [
            line(1, "GET", "/a"),
            line(1, "GET", "/a"),
            line(2, "GET", "/a"),
            line(2, "POST", "/b"),
        ]
        .join("\n");
        let c = convert_accesslog(&log).expect("convert");
        let r = random_step(&c);
        assert_eq!(r.strategy, SwitchStrategy::Weighted);
        assert_eq!(r.choices.len(), 2);
        assert_eq!(r.choices[0].name.as_deref(), Some("GET /a"));
        assert_eq!(r.choices[0].weight, Some(3.0));
        assert_eq!(r.choices[1].weight, Some(1.0));
        let req = match &r.choices[1].steps[0] {
            Step::Request(q) => q,
            other => panic!("expected request step, got {other:?}"),
        };
        assert_eq!(req.method.as_deref(), Some("POST"));
        assert_eq!(req.url, "/b");
        // POST body is unknowable from a log.
        assert!(c
            .warnings
            .iter()
            .any(|w| w.message.contains("request bodies")));
    }

    #[test]
    fn caps_the_weighted_mix_at_top_n_endpoints() {
        let log = [
            line(1, "GET", "/a"),
            line(1, "GET", "/a"),
            line(1, "GET", "/a"),
            line(2, "GET", "/b"),
            line(2, "GET", "/b"),
            line(3, "GET", "/c"),
        ]
        .join("\n");
        let c = convert_with_top_n(&log, 2).expect("convert");
        let r = random_step(&c);
        assert_eq!(r.choices.len(), 2);
        assert_eq!(r.choices[0].name.as_deref(), Some("GET /a"));
        assert_eq!(r.choices[1].name.as_deref(), Some("GET /b"));
        assert!(c.warnings.iter().any(|w| w.message.contains("top 2 of 3")));
    }

    #[test]
    fn rejects_logs_with_no_parseable_lines() {
        assert!(matches!(
            convert_accesslog(""),
            Err(ConvertError::AccessLog(_))
        ));
        assert!(matches!(
            convert_accesslog("not a log\nalso not a log\n"),
            Err(ConvertError::AccessLog(_))
        ));
    }

    #[test]
    fn plan_round_trips_and_validates() {
        let log = [
            line(1, "GET", "/users/42"),
            line(2, "POST", "/orders"),
            line(3, "GET", "/health"),
        ]
        .join("\n");
        let c = convert_accesslog(&log).expect("convert");
        let yaml = serde_yaml::to_string(&c.plan).unwrap();
        // Round-trips through the schema and validates without errors.
        let opts = loadr_config::LoadOptions {
            check_files: false,
            deny_errors: true,
            ..Default::default()
        };
        loadr_config::load_str(&yaml, &opts).expect("converted plan should be valid");
    }
}
