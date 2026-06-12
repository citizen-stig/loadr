//! Threshold expression parsing: `p(95)<400`, `rate>0.99`, `avg<200`, ...
//!
//! A threshold key may carry a tag selector: `http_req_duration{scenario:browse}`.

use std::fmt;

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ThresholdParseError {
    #[error("empty threshold expression")]
    Empty,
    #[error(
        "unknown aggregation `{0}` (expected one of: avg, min, max, med, p(N), rate, count, value)"
    )]
    UnknownAgg(String),
    #[error("invalid percentile `{0}`: must be a number in (0, 100]")]
    BadPercentile(String),
    #[error("missing comparison operator in `{0}` (expected <, <=, >, >=, ==, or !=)")]
    MissingOp(String),
    #[error("invalid numeric bound `{0}`")]
    BadNumber(String),
    #[error("invalid metric selector `{0}`: {1}")]
    BadSelector(String, String),
}

/// Aggregation over a metric's samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Agg {
    Avg,
    Min,
    Max,
    Med,
    /// Percentile in (0, 100].
    Percentile(f64),
    /// For rate metrics: fraction of non-zero samples.
    Rate,
    /// For counters: total count.
    Count,
    /// For gauges: last value.
    Value,
}

impl fmt::Display for Agg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Agg::Avg => write!(f, "avg"),
            Agg::Min => write!(f, "min"),
            Agg::Max => write!(f, "max"),
            Agg::Med => write!(f, "med"),
            Agg::Percentile(p) => write!(f, "p({p})"),
            Agg::Rate => write!(f, "rate"),
            Agg::Count => write!(f, "count"),
            Agg::Value => write!(f, "value"),
        }
    }
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Op {
    pub fn eval(&self, lhs: f64, rhs: f64) -> bool {
        match self {
            Op::Lt => lhs < rhs,
            Op::Le => lhs <= rhs,
            Op::Gt => lhs > rhs,
            Op::Ge => lhs >= rhs,
            Op::Eq => (lhs - rhs).abs() < f64::EPSILON,
            Op::Ne => (lhs - rhs).abs() >= f64::EPSILON,
        }
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Eq => "==",
            Op::Ne => "!=",
        };
        f.write_str(s)
    }
}

/// A parsed threshold expression: `<agg> <op> <bound>`.
#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdExpr {
    pub agg: Agg,
    pub op: Op,
    pub bound: f64,
}

impl ThresholdExpr {
    pub fn parse(s: &str) -> Result<Self, ThresholdParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ThresholdParseError::Empty);
        }
        // Find the operator. Two-char ops first.
        let ops: &[(&str, Op)] = &[
            ("<=", Op::Le),
            (">=", Op::Ge),
            ("==", Op::Eq),
            ("!=", Op::Ne),
            ("<", Op::Lt),
            (">", Op::Gt),
            ("=", Op::Eq),
        ];
        let mut found: Option<(usize, usize, Op)> = None;
        for (tok, op) in ops {
            if let Some(idx) = s.find(tok) {
                match found {
                    Some((fidx, _, _)) if fidx <= idx => {}
                    _ => found = Some((idx, tok.len(), *op)),
                }
            }
        }
        let (idx, len, op) = found.ok_or_else(|| ThresholdParseError::MissingOp(s.to_string()))?;
        let lhs = s[..idx].trim();
        let rhs = s[idx + len..].trim();

        let agg = parse_agg(lhs)?;
        let bound = parse_bound(rhs)?;
        Ok(ThresholdExpr { agg, op, bound })
    }
}

impl fmt::Display for ThresholdExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.agg, self.op, self.bound)
    }
}

fn parse_agg(s: &str) -> Result<Agg, ThresholdParseError> {
    match s {
        "avg" => Ok(Agg::Avg),
        "min" => Ok(Agg::Min),
        "max" => Ok(Agg::Max),
        "med" => Ok(Agg::Med),
        "rate" => Ok(Agg::Rate),
        "count" => Ok(Agg::Count),
        "value" => Ok(Agg::Value),
        _ => {
            if let Some(inner) = s.strip_prefix("p(").and_then(|rest| rest.strip_suffix(')')) {
                let p: f64 = inner
                    .trim()
                    .parse()
                    .map_err(|_| ThresholdParseError::BadPercentile(inner.to_string()))?;
                if p <= 0.0 || p > 100.0 {
                    return Err(ThresholdParseError::BadPercentile(inner.to_string()));
                }
                Ok(Agg::Percentile(p))
            } else {
                Err(ThresholdParseError::UnknownAgg(s.to_string()))
            }
        }
    }
}

fn parse_bound(s: &str) -> Result<f64, ThresholdParseError> {
    // Allow duration-style bounds for time metrics: `p(95)<400ms`, `<1.5s`.
    if let Ok(v) = s.parse::<f64>() {
        return Ok(v);
    }
    if let Ok(d) = crate::duration::Dur::parse(s) {
        return Ok(d.as_duration().as_secs_f64() * 1000.0); // milliseconds
    }
    Err(ThresholdParseError::BadNumber(s.to_string()))
}

/// A metric selector: a metric name plus optional tag filters.
/// Syntax: `name` or `name{tag:value,tag2:value2}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricSelector {
    pub metric: String,
    pub tags: Vec<(String, String)>,
}

impl MetricSelector {
    pub fn parse(s: &str) -> Result<Self, ThresholdParseError> {
        let s = s.trim();
        if let Some(open) = s.find('{') {
            let close = s.rfind('}').ok_or_else(|| {
                ThresholdParseError::BadSelector(s.to_string(), "missing closing `}`".into())
            })?;
            if close < open {
                return Err(ThresholdParseError::BadSelector(
                    s.to_string(),
                    "`}` before `{`".into(),
                ));
            }
            let metric = s[..open].trim().to_string();
            if metric.is_empty() {
                return Err(ThresholdParseError::BadSelector(
                    s.to_string(),
                    "empty metric name".into(),
                ));
            }
            let mut tags = Vec::new();
            for pair in s[open + 1..close].split(',') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                let (k, v) = pair.split_once(':').ok_or_else(|| {
                    ThresholdParseError::BadSelector(
                        s.to_string(),
                        format!("tag `{pair}` must be `key:value`"),
                    )
                })?;
                tags.push((k.trim().to_string(), v.trim().to_string()));
            }
            tags.sort();
            Ok(MetricSelector { metric, tags })
        } else {
            if s.is_empty() {
                return Err(ThresholdParseError::BadSelector(
                    s.to_string(),
                    "empty metric name".into(),
                ));
            }
            Ok(MetricSelector {
                metric: s.to_string(),
                tags: Vec::new(),
            })
        }
    }
}

impl fmt::Display for MetricSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.metric)?;
        if !self.tags.is_empty() {
            write!(f, "{{")?;
            for (i, (k, v)) in self.tags.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }
                write!(f, "{k}:{v}")?;
            }
            write!(f, "}}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_percentiles() {
        let t = ThresholdExpr::parse("p(95)<400").unwrap();
        assert_eq!(t.agg, Agg::Percentile(95.0));
        assert_eq!(t.op, Op::Lt);
        assert_eq!(t.bound, 400.0);

        let t = ThresholdExpr::parse("p(99.9) <= 1200").unwrap();
        assert_eq!(t.agg, Agg::Percentile(99.9));
        assert_eq!(t.op, Op::Le);
    }

    #[test]
    fn parses_all_aggs() {
        for (s, agg) in [
            ("avg<1", Agg::Avg),
            ("min>1", Agg::Min),
            ("max<1", Agg::Max),
            ("med<1", Agg::Med),
            ("rate>0.95", Agg::Rate),
            ("count>100", Agg::Count),
            ("value<5", Agg::Value),
        ] {
            assert_eq!(ThresholdExpr::parse(s).unwrap().agg, agg, "{s}");
        }
    }

    #[test]
    fn duration_bounds_become_millis() {
        let t = ThresholdExpr::parse("p(95)<400ms").unwrap();
        assert_eq!(t.bound, 400.0);
        let t = ThresholdExpr::parse("avg<1.5s").unwrap();
        assert_eq!(t.bound, 1500.0);
    }

    #[test]
    fn error_cases() {
        assert!(matches!(
            ThresholdExpr::parse("p(101)<1"),
            Err(ThresholdParseError::BadPercentile(_))
        ));
        assert!(matches!(
            ThresholdExpr::parse("p95<1"),
            Err(ThresholdParseError::UnknownAgg(_))
        ));
        assert!(matches!(
            ThresholdExpr::parse("avg 400"),
            Err(ThresholdParseError::MissingOp(_))
        ));
        assert!(matches!(
            ThresholdExpr::parse("avg<fast"),
            Err(ThresholdParseError::BadNumber(_))
        ));
    }

    #[test]
    fn selector_with_tags() {
        let sel = MetricSelector::parse("http_req_duration{scenario:browse, name:home}").unwrap();
        assert_eq!(sel.metric, "http_req_duration");
        assert_eq!(
            sel.tags,
            vec![
                ("name".to_string(), "home".to_string()),
                ("scenario".to_string(), "browse".to_string()),
            ]
        );
        assert_eq!(
            sel.to_string(),
            "http_req_duration{name:home,scenario:browse}"
        );
    }

    #[test]
    fn selector_plain() {
        let sel = MetricSelector::parse("checks").unwrap();
        assert_eq!(sel.metric, "checks");
        assert!(sel.tags.is_empty());
    }

    #[test]
    fn op_eval() {
        assert!(Op::Lt.eval(1.0, 2.0));
        assert!(!Op::Lt.eval(2.0, 2.0));
        assert!(Op::Le.eval(2.0, 2.0));
        assert!(Op::Ge.eval(2.0, 2.0));
        assert!(Op::Ne.eval(1.0, 2.0));
        assert!(Op::Eq.eval(2.0, 2.0));
    }
}
