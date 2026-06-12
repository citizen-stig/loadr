//! Friendly diagnostics: severities, positions, did-you-mean suggestions, and a
//! best-effort YAML span index mapping document paths to line/column.

use std::fmt;

/// Severity of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warning,
    Error,
}

/// A single validation finding.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Dotted document path, e.g. `scenarios.browse.flow[0].request.url`.
    pub path: String,
    pub message: String,
    /// 1-based line, if known.
    pub line: Option<usize>,
    /// 1-based column, if known.
    pub column: Option<usize>,
    /// "did you mean ..." style hint.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn error(path: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            path: path.into(),
            message: message.into(),
            line: None,
            column: None,
            suggestion: None,
        }
    }

    pub fn warning(path: impl Into<String>, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(path, message)
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_position(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// Attach a position looked up from the span index, when available.
    pub fn locate(mut self, index: &SpanIndex) -> Self {
        if self.line.is_none() {
            if let Some((line, col)) = index.lookup(&self.path) {
                self.line = Some(line);
                self.column = Some(col);
            }
        }
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "{sev} at line {l}, column {c}")?,
            (Some(l), None) => write!(f, "{sev} at line {l}")?,
            _ => write!(f, "{sev}")?,
        }
        if !self.path.is_empty() {
            write!(f, " ({})", self.path)?;
        }
        write!(f, ": {}", self.message)?;
        if let Some(s) = &self.suggestion {
            write!(f, " — {s}")?;
        }
        Ok(())
    }
}

/// Pick the closest candidate to `input` for a did-you-mean hint.
pub fn did_you_mean<'a, I>(input: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut best: Option<(f64, &str)> = None;
    for cand in candidates {
        let score = strsim::jaro_winkler(input, cand);
        if score > best.map(|(s, _)| s).unwrap_or(0.0) {
            best = Some((score, cand));
        }
    }
    best.filter(|(score, _)| *score >= 0.78)
        .map(|(_, cand)| format!("did you mean `{cand}`?"))
}

/// A best-effort index from YAML document paths to source positions.
///
/// Built with a lightweight indentation-based scan (not a full YAML parser), so
/// it handles the dominant block style: nested mappings and `- ` sequences.
/// Flow style (`{a: 1}`) resolves to the line of the containing key.
#[derive(Debug, Default)]
pub struct SpanIndex {
    /// (path, line, column), 1-based.
    entries: Vec<(String, usize, usize)>,
}

impl SpanIndex {
    pub fn build(source: &str) -> SpanIndex {
        let mut entries = Vec::new();
        // Stack of (indent, path_segment, seq_counter)
        let mut stack: Vec<(usize, String, Option<usize>)> = Vec::new();

        for (lineno, raw) in source.lines().enumerate() {
            let line_no = lineno + 1;
            let trimmed_start = raw.trim_start();
            if trimmed_start.is_empty() || trimmed_start.starts_with('#') {
                continue;
            }
            let mut indent = raw.len() - trimmed_start.len();
            let mut rest = trimmed_start;
            // Pop deeper or equal levels.
            while let Some((top_indent, _, _)) = stack.last() {
                if *top_indent >= indent {
                    stack.pop();
                } else {
                    break;
                }
            }
            // Handle sequence dashes, possibly several (`- - x`) but typically one.
            while let Some(after) =
                rest.strip_prefix("- ")
                    .or(if rest == "-" { Some("") } else { None })
            {
                // Sequence item under current path: find the parent seq counter.
                let parent_path = Self::path_of(&stack);
                let counter = entries
                    .iter()
                    .rev()
                    .find_map(|(p, _, _): &(String, usize, usize)| {
                        let prefix = format!("{parent_path}[");
                        if p.starts_with(&prefix) && !p[prefix.len()..].contains(['.', '[']) {
                            p[prefix.len()..]
                                .strip_suffix(']')
                                .and_then(|n| n.parse::<usize>().ok())
                        } else {
                            None
                        }
                    })
                    .map(|n| n + 1)
                    .unwrap_or(0);
                let seg = format!("[{counter}]");
                let col = indent + 1;
                let item_path = format!("{parent_path}{seg}");
                entries.push((item_path, line_no, col));
                stack.push((indent, seg, None));
                indent += 2;
                rest = after.trim_start();
                if rest.is_empty() {
                    break;
                }
            }
            if rest.is_empty() {
                continue;
            }
            // Mapping key?
            if let Some(colon) = find_key_colon(rest) {
                let key = rest[..colon].trim().trim_matches('"').trim_matches('\'');
                if !key.is_empty() {
                    let col = raw.len() - rest.len() + 1;
                    let mut path = Self::path_of(&stack);
                    if !path.is_empty() && !key.starts_with('[') {
                        path.push('.');
                    }
                    path.push_str(key);
                    entries.push((path, line_no, col));
                    stack.push((indent, key.to_string(), None));
                }
            }
        }
        SpanIndex { entries }
    }

    fn path_of(stack: &[(usize, String, Option<usize>)]) -> String {
        let mut out = String::new();
        for (_, seg, _) in stack {
            if seg.starts_with('[') {
                out.push_str(seg);
            } else {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(seg);
            }
        }
        out
    }

    /// Look up a path, falling back to progressively shorter prefixes.
    pub fn lookup(&self, path: &str) -> Option<(usize, usize)> {
        let mut candidate = path.to_string();
        loop {
            if let Some((_, l, c)) = self.entries.iter().find(|(p, _, _)| *p == candidate) {
                return Some((*l, *c));
            }
            // Trim the last segment (`.seg` or `[n]`).
            if let Some(idx) = candidate.rfind(['.', '[']) {
                candidate.truncate(idx);
                if candidate.is_empty() {
                    return None;
                }
            } else {
                return None;
            }
        }
    }
}

/// Find the colon that terminates a YAML key on this line, skipping quoted keys
/// and colons inside flow context.
fn find_key_colon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'{' | b'[' if !in_single && !in_double => depth += 1,
            b'}' | b']' if !in_single && !in_double => depth = depth.saturating_sub(1),
            b':' if !in_single && !in_double && depth == 0 => {
                // A key colon is followed by space/EOL.
                if i + 1 >= bytes.len() || bytes[i + 1] == b' ' {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"name: demo
defaults:
  http:
    base_url: https://example.com
scenarios:
  browse:
    executor: constant-vus
    flow:
      - request:
          url: /
      - think_time: { type: constant, duration: 1s }
"#;

    #[test]
    fn lookup_nested_keys() {
        let idx = SpanIndex::build(DOC);
        assert_eq!(idx.lookup("name"), Some((1, 1)));
        assert_eq!(idx.lookup("defaults.http.base_url"), Some((4, 5)));
        assert_eq!(idx.lookup("scenarios.browse.executor"), Some((7, 5)));
    }

    #[test]
    fn lookup_sequence_items() {
        let idx = SpanIndex::build(DOC);
        let (line, _) = idx.lookup("scenarios.browse.flow[0].request.url").unwrap();
        assert_eq!(line, 10);
        let (line, _) = idx.lookup("scenarios.browse.flow[1]").unwrap();
        assert_eq!(line, 11);
    }

    #[test]
    fn lookup_falls_back_to_prefix() {
        let idx = SpanIndex::build(DOC);
        // Unknown leaf falls back to the nearest known ancestor.
        let (line, _) = idx
            .lookup("scenarios.browse.flow[0].request.nonexistent")
            .unwrap();
        assert_eq!(line, 9);
    }

    #[test]
    fn did_you_mean_suggests_close_match() {
        let s = did_you_mean("scenarois", ["scenarios", "thresholds", "outputs"]);
        assert_eq!(s.as_deref(), Some("did you mean `scenarios`?"));
        assert!(did_you_mean("zzz", ["scenarios"]).is_none());
    }

    #[test]
    fn diagnostic_display() {
        let d = Diagnostic::error("scenarios.x", "missing `vus`")
            .with_position(7, 5)
            .with_suggestion("add `vus: 10`");
        let s = d.to_string();
        assert!(s.contains("line 7"));
        assert!(s.contains("scenarios.x"));
        assert!(s.contains("add `vus: 10`"));
    }
}
