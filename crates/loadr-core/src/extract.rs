//! Correlation/extraction: pull values out of responses into VU variables.
//!
//! Two flavours share this module:
//! - the **classic** single-purpose extractors ([`CompiledClassic`]);
//! - the **fused chain** ([`CompiledChain`]) that, in one step, extracts a
//!   value, optionally coerces its type, runs a small transform pipeline,
//!   validates it with inline checks and saves it.

use base64::Engine as _;
use loadr_config::{
    ChainCheck, ChainSpec, ClassicExtractor, CoerceType, Extractor, FailureAction, MatchIndex,
    Transform,
};
use rand::RngExt;

use crate::protocol::ProtocolResponse;

/// A compiled extractor: either a classic single-source extractor or a fused
/// chain. Both yield a JSON value and a variable name.
#[derive(Debug)]
pub enum CompiledExtractor {
    Classic(CompiledClassic),
    Chain(CompiledChain),
}

/// A compiled classic extractor (regexes/paths parsed once at plan compile time).
#[derive(Debug)]
pub enum CompiledClassic {
    Jsonpath {
        name: String,
        path: serde_json_path::JsonPath,
        default: Option<String>,
        index: MatchIndex,
    },
    Regex {
        name: String,
        regex: regex::Regex,
        group: usize,
        default: Option<String>,
        index: MatchIndex,
    },
    Xpath {
        name: String,
        expression: String,
        default: Option<String>,
    },
    Css {
        name: String,
        selector: scraper::Selector,
        attribute: Option<String>,
        default: Option<String>,
        index: MatchIndex,
    },
    Boundary {
        name: String,
        left: String,
        right: String,
        default: Option<String>,
        index: MatchIndex,
    },
    Header {
        name: String,
        header: String,
        default: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("extractor `{0}` is invalid: {1}")]
    Invalid(String, String),
    #[error("extractor `{name}` found no match and has no default")]
    NoMatch { name: String },
    /// A chain's inline `check` rejected the extracted value.
    #[error("chain `{name}` failed validation: {detail}")]
    CheckFailed {
        name: String,
        detail: String,
        on_failure: FailureAction,
    },
}

impl CompiledExtractor {
    pub fn compile(spec: &Extractor) -> Result<Self, ExtractError> {
        Ok(match spec {
            Extractor::Classic(c) => CompiledExtractor::Classic(CompiledClassic::compile(c)?),
            Extractor::Chain(c) => CompiledExtractor::Chain(CompiledChain::compile(c)?),
        })
    }

    pub fn name(&self) -> &str {
        match self {
            CompiledExtractor::Classic(c) => c.name(),
            CompiledExtractor::Chain(c) => &c.name,
        }
    }

    /// True for a fused chain that carries an inline `check:` block. Such
    /// chains record a sample to the `checks` metric (like standalone checks).
    pub fn is_chain_with_check(&self) -> bool {
        matches!(self, CompiledExtractor::Chain(c) if c.check.is_some())
    }

    /// Run the extractor; returns the extracted (and, for chains, coerced /
    /// transformed / validated) value.
    pub fn extract(
        &self,
        response: &ProtocolResponse,
        rng: &mut impl RngExt,
    ) -> Result<serde_json::Value, ExtractError> {
        match self {
            CompiledExtractor::Classic(c) => c.extract(response, rng),
            CompiledExtractor::Chain(c) => c.extract(response, rng),
        }
    }
}

impl CompiledClassic {
    pub fn compile(spec: &ClassicExtractor) -> Result<Self, ExtractError> {
        Ok(match spec {
            ClassicExtractor::Jsonpath {
                name,
                expression,
                default,
                index,
            } => CompiledClassic::Jsonpath {
                name: name.clone(),
                path: serde_json_path::JsonPath::parse(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            ClassicExtractor::Regex {
                name,
                expression,
                group,
                default,
                index,
            } => CompiledClassic::Regex {
                name: name.clone(),
                regex: regex::Regex::new(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                group: group.unwrap_or(1),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            ClassicExtractor::Xpath {
                name,
                expression,
                default,
            } => CompiledClassic::Xpath {
                name: name.clone(),
                expression: expression.clone(),
                default: default.clone(),
            },
            ClassicExtractor::Css {
                name,
                expression,
                attribute,
                default,
                index,
            } => CompiledClassic::Css {
                name: name.clone(),
                selector: scraper::Selector::parse(expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e.to_string()))?,
                attribute: attribute.clone(),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            ClassicExtractor::Boundary {
                name,
                left,
                right,
                default,
                index,
            } => CompiledClassic::Boundary {
                name: name.clone(),
                left: left.clone(),
                right: right.clone(),
                default: default.clone(),
                index: index.unwrap_or_default(),
            },
            ClassicExtractor::Header {
                name,
                header,
                default,
            } => CompiledClassic::Header {
                name: name.clone(),
                header: header.clone(),
                default: default.clone(),
            },
        })
    }

    pub fn name(&self) -> &str {
        match self {
            CompiledClassic::Jsonpath { name, .. }
            | CompiledClassic::Regex { name, .. }
            | CompiledClassic::Xpath { name, .. }
            | CompiledClassic::Css { name, .. }
            | CompiledClassic::Boundary { name, .. }
            | CompiledClassic::Header { name, .. } => name,
        }
    }

    fn default(&self) -> Option<&str> {
        match self {
            CompiledClassic::Jsonpath { default, .. }
            | CompiledClassic::Regex { default, .. }
            | CompiledClassic::Xpath { default, .. }
            | CompiledClassic::Css { default, .. }
            | CompiledClassic::Boundary { default, .. }
            | CompiledClassic::Header { default, .. } => default.as_deref(),
        }
    }

    /// Run the extractor; returns the extracted value as a JSON value
    /// (JSONPath keeps native types; everything else yields strings).
    pub fn extract(
        &self,
        response: &ProtocolResponse,
        rng: &mut impl RngExt,
    ) -> Result<serde_json::Value, ExtractError> {
        let result: Option<serde_json::Value> = match self {
            CompiledClassic::Jsonpath { path, index, .. } => {
                let body: serde_json::Value =
                    serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null);
                let nodes = path.query(&body);
                let all: Vec<serde_json::Value> = nodes.iter().map(|v| (*v).clone()).collect();
                pick(all, *index, rng)
            }
            CompiledClassic::Regex {
                regex,
                group,
                index,
                ..
            } => {
                let text = response.body_text();
                let all: Vec<serde_json::Value> = regex
                    .captures_iter(&text)
                    .filter_map(|c| {
                        c.get(*group)
                            .map(|m| serde_json::Value::String(m.as_str().to_string()))
                    })
                    .collect();
                pick(all, *index, rng)
            }
            CompiledClassic::Xpath {
                name, expression, ..
            } => {
                let text = response.body_text();
                xpath_eval(&text, expression)
                    .map_err(|e| ExtractError::Invalid(name.clone(), e))?
                    .map(serde_json::Value::String)
            }
            CompiledClassic::Css {
                selector,
                attribute,
                index,
                ..
            } => {
                let text = response.body_text();
                let doc = scraper::Html::parse_document(&text);
                let all: Vec<serde_json::Value> = doc
                    .select(selector)
                    .filter_map(|el| match attribute {
                        Some(attr) => el.attr(attr).map(str::to_string),
                        None => Some(el.text().collect::<String>()),
                    })
                    .map(serde_json::Value::String)
                    .collect();
                pick(all, *index, rng)
            }
            CompiledClassic::Boundary {
                left, right, index, ..
            } => {
                let text = response.body_text();
                pick(boundary_matches(&text, left, right), *index, rng)
            }
            CompiledClassic::Header { header, .. } => response
                .header(header)
                .map(|v| serde_json::Value::String(v.to_string())),
        };

        match result {
            Some(v) => Ok(v),
            None => match self.default() {
                Some(d) => Ok(serde_json::Value::String(d.to_string())),
                None => Err(ExtractError::NoMatch {
                    name: self.name().to_string(),
                }),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Fused chains
// ---------------------------------------------------------------------------

/// The compiled source half of a chain (one of several extractor backends).
#[derive(Debug)]
enum ChainSource {
    Jmespath(jmespath::Expression<'static>),
    Jsonpath(serde_json_path::JsonPath),
    Regex {
        regex: regex::Regex,
        group: usize,
    },
    Header(String),
    Css {
        selector: scraper::Selector,
        attribute: Option<String>,
    },
    Xpath(String),
    Boundary {
        left: String,
        right: String,
    },
}

/// A fully compiled fused chain (extract → coerce → transform → validate → save).
#[derive(Debug)]
pub struct CompiledChain {
    pub name: String,
    source: ChainSource,
    index: MatchIndex,
    coerce: Option<CoerceType>,
    transforms: Vec<Transform>,
    default: Option<serde_json::Value>,
    check: Option<CompiledChainCheck>,
}

#[derive(Debug)]
struct CompiledChainCheck {
    equals: Option<serde_json::Value>,
    matches: Option<regex::Regex>,
    one_of: Option<Vec<serde_json::Value>>,
    min: Option<f64>,
    max: Option<f64>,
    not_empty: bool,
    on_failure: FailureAction,
}

impl CompiledChain {
    pub fn compile(spec: &ChainSpec) -> Result<Self, ExtractError> {
        let name = spec.name.clone();
        let invalid = |msg: String| ExtractError::Invalid(name.clone(), msg);

        let source = if let Some(expr) = &spec.jmespath {
            ChainSource::Jmespath(jmespath::compile(expr).map_err(|e| invalid(e.to_string()))?)
        } else if let Some(expr) = &spec.jsonpath {
            ChainSource::Jsonpath(
                serde_json_path::JsonPath::parse(expr).map_err(|e| invalid(e.to_string()))?,
            )
        } else if let Some(expr) = &spec.regex {
            ChainSource::Regex {
                regex: regex::Regex::new(expr).map_err(|e| invalid(e.to_string()))?,
                group: spec.group.unwrap_or(1),
            }
        } else if let Some(h) = &spec.header {
            ChainSource::Header(h.clone())
        } else if let Some(sel) = &spec.css {
            ChainSource::Css {
                selector: scraper::Selector::parse(sel).map_err(|e| invalid(e.to_string()))?,
                attribute: spec.attribute.clone(),
            }
        } else if let Some(expr) = &spec.xpath {
            ChainSource::Xpath(expr.clone())
        } else if let (Some(left), Some(right)) = (&spec.left, &spec.right) {
            ChainSource::Boundary {
                left: left.clone(),
                right: right.clone(),
            }
        } else {
            return Err(invalid("chain has no source".to_string()));
        };

        let check = match &spec.check {
            Some(c) => Some(CompiledChainCheck::compile(&name, c)?),
            None => None,
        };

        Ok(CompiledChain {
            name,
            source,
            index: spec.index.unwrap_or_default(),
            coerce: spec.coerce(),
            transforms: spec.transform.clone(),
            default: spec.default.clone(),
            check,
        })
    }

    fn extract(
        &self,
        response: &ProtocolResponse,
        rng: &mut impl RngExt,
    ) -> Result<serde_json::Value, ExtractError> {
        // 1. Extract raw value(s).
        let raw = self.extract_raw(response, rng);

        // 2. Apply default when nothing matched.
        let mut value = match raw {
            Some(v) => v,
            None => match &self.default {
                Some(d) => d.clone(),
                None => {
                    return Err(ExtractError::NoMatch {
                        name: self.name.clone(),
                    })
                }
            },
        };

        // 3. Coerce type.
        if let Some(ct) = self.coerce {
            value = coerce(&self.name, value, ct)?;
        }

        // 4. Transform pipeline.
        for t in &self.transforms {
            value = apply_transform(&self.name, value, t)?;
        }

        // 5. Validate.
        if let Some(check) = &self.check {
            if let Err(detail) = check.evaluate(&value) {
                return Err(ExtractError::CheckFailed {
                    name: self.name.clone(),
                    detail,
                    on_failure: check.on_failure,
                });
            }
        }

        Ok(value)
    }

    fn extract_raw(
        &self,
        response: &ProtocolResponse,
        rng: &mut impl RngExt,
    ) -> Option<serde_json::Value> {
        match &self.source {
            ChainSource::Jmespath(expr) => {
                let var = jmespath::Variable::from_json(&response.body_text()).ok()?;
                let result = expr.search(var).ok()?;
                if result.is_null() {
                    return None;
                }
                let value: serde_json::Value =
                    serde_json::to_value(&*result).unwrap_or(serde_json::Value::Null);
                // For arrays, honour `index` like the other multi-match sources.
                match value {
                    serde_json::Value::Array(items) if self.index != MatchIndex::All => {
                        pick(items, self.index, rng)
                    }
                    other => Some(other),
                }
            }
            ChainSource::Jsonpath(path) => {
                let body: serde_json::Value =
                    serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null);
                let all: Vec<serde_json::Value> =
                    path.query(&body).iter().map(|v| (*v).clone()).collect();
                pick(all, self.index, rng)
            }
            ChainSource::Regex { regex, group } => {
                let text = response.body_text();
                let all: Vec<serde_json::Value> = regex
                    .captures_iter(&text)
                    .filter_map(|c| {
                        c.get(*group)
                            .map(|m| serde_json::Value::String(m.as_str().to_string()))
                    })
                    .collect();
                pick(all, self.index, rng)
            }
            ChainSource::Header(header) => response
                .header(header)
                .map(|v| serde_json::Value::String(v.to_string())),
            ChainSource::Css {
                selector,
                attribute,
            } => {
                let text = response.body_text();
                let doc = scraper::Html::parse_document(&text);
                let all: Vec<serde_json::Value> = doc
                    .select(selector)
                    .filter_map(|el| match attribute {
                        Some(attr) => el.attr(attr).map(str::to_string),
                        None => Some(el.text().collect::<String>()),
                    })
                    .map(serde_json::Value::String)
                    .collect();
                pick(all, self.index, rng)
            }
            ChainSource::Xpath(expr) => {
                let text = response.body_text();
                xpath_eval(&text, expr)
                    .ok()
                    .flatten()
                    .map(serde_json::Value::String)
            }
            ChainSource::Boundary { left, right } => {
                let text = response.body_text();
                pick(boundary_matches(&text, left, right), self.index, rng)
            }
        }
    }
}

impl CompiledChainCheck {
    fn compile(name: &str, c: &ChainCheck) -> Result<Self, ExtractError> {
        Ok(CompiledChainCheck {
            equals: c.equals.clone(),
            matches: c
                .matches
                .as_ref()
                .map(|m| regex::Regex::new(m))
                .transpose()
                .map_err(|e| ExtractError::Invalid(name.to_string(), e.to_string()))?,
            one_of: c.one_of.clone(),
            min: c.min,
            max: c.max,
            not_empty: c.not_empty.unwrap_or(false),
            on_failure: c.on_failure.unwrap_or_default(),
        })
    }

    fn evaluate(&self, value: &serde_json::Value) -> Result<(), String> {
        let as_text = value_to_text(value);
        if let Some(expected) = &self.equals {
            if value != expected {
                return Err(format!("expected {expected}, got {value}"));
            }
        }
        if let Some(re) = &self.matches {
            if !re.is_match(&as_text) {
                return Err(format!("{as_text:?} does not match /{re}/"));
            }
        }
        if let Some(set) = &self.one_of {
            let ok = set
                .iter()
                .any(|c| c == value || value_to_text(c) == as_text);
            if !ok {
                return Err(format!("{as_text:?} is not one of {set:?}"));
            }
        }
        if self.min.is_some() || self.max.is_some() {
            let n =
                value_as_f64(value).ok_or_else(|| format!("value {as_text:?} is not numeric"))?;
            if let Some(min) = self.min {
                if n < min {
                    return Err(format!("{n} is below minimum {min}"));
                }
            }
            if let Some(max) = self.max {
                if n > max {
                    return Err(format!("{n} is above maximum {max}"));
                }
            }
        }
        if self.not_empty && as_text.is_empty() {
            return Err("value is empty".to_string());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Coercion & transforms
// ---------------------------------------------------------------------------

fn coerce(
    name: &str,
    value: serde_json::Value,
    ct: CoerceType,
) -> Result<serde_json::Value, ExtractError> {
    let bad = |msg: String| ExtractError::Invalid(name.to_string(), msg);
    let text = value_to_text(&value);
    Ok(match ct {
        CoerceType::String => serde_json::Value::String(text),
        CoerceType::Int => {
            // Accept ints, floats that are whole numbers, and numeric strings.
            let n = if let Some(i) = value.as_i64() {
                i
            } else if let Some(f) = value.as_f64() {
                f as i64
            } else {
                text.trim()
                    .parse::<i64>()
                    .map_err(|_| bad(format!("cannot coerce {text:?} to int")))?
            };
            serde_json::Value::Number(n.into())
        }
        CoerceType::Float => {
            let f = value
                .as_f64()
                .or_else(|| text.trim().parse::<f64>().ok())
                .ok_or_else(|| bad(format!("cannot coerce {text:?} to float")))?;
            serde_json::Number::from_f64(f)
                .map(serde_json::Value::Number)
                .ok_or_else(|| bad(format!("{f} is not a finite float")))?
        }
        CoerceType::Bool => {
            let b = match value {
                serde_json::Value::Bool(b) => b,
                _ => match text.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" | "on" => true,
                    "false" | "0" | "no" | "off" => false,
                    _ => return Err(bad(format!("cannot coerce {text:?} to bool"))),
                },
            };
            serde_json::Value::Bool(b)
        }
    })
}

fn apply_transform(
    name: &str,
    value: serde_json::Value,
    t: &Transform,
) -> Result<serde_json::Value, ExtractError> {
    let bad = |msg: String| ExtractError::Invalid(name.to_string(), msg);
    let text = value_to_text(&value);
    let out = match t {
        Transform::Trim => text.trim().to_string(),
        Transform::Lowercase => text.to_lowercase(),
        Transform::Uppercase => text.to_uppercase(),
        Transform::UrlEncode => url_encode(&text),
        Transform::UrlDecode => url_decode(&text),
        Transform::Base64Encode => {
            base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
        }
        Transform::Base64Decode => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text.as_bytes())
                .map_err(|e| bad(format!("base64 decode: {e}")))?;
            String::from_utf8(bytes).map_err(|e| bad(format!("base64 decode: {e}")))?
        }
        Transform::Append(s) => format!("{text}{s}"),
        Transform::Prepend(s) => format!("{s}{text}"),
        Transform::Replace(args) => {
            if args.len() != 2 {
                return Err(bad("`replace` needs [from, to]".to_string()));
            }
            text.replace(&args[0], &args[1])
        }
        Transform::Substring(args) => {
            let chars: Vec<char> = text.chars().collect();
            let start = (*args.first().unwrap_or(&0)).min(chars.len());
            let end = match args.get(1) {
                Some(len) => (start + len).min(chars.len()),
                None => chars.len(),
            };
            chars[start..end.max(start)].iter().collect()
        }
    };
    Ok(serde_json::Value::String(out))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Render a JSON value as the text other steps interpolate (`${name}`).
fn value_to_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn value_as_f64(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        serde_json::Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Minimal percent-encoding of everything outside the unreserved set.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Collect every value between `left` and `right` markers.
fn boundary_matches(text: &str, left: &str, right: &str) -> Vec<serde_json::Value> {
    let mut all = Vec::new();
    let mut at = 0usize;
    while let Some(start) = text[at..].find(left) {
        let vstart = at + start + left.len();
        match text[vstart..].find(right) {
            Some(end) => {
                all.push(serde_json::Value::String(
                    text[vstart..vstart + end].to_string(),
                ));
                at = vstart + end + right.len();
            }
            None => break,
        }
    }
    all
}

fn pick(
    mut all: Vec<serde_json::Value>,
    index: MatchIndex,
    rng: &mut impl RngExt,
) -> Option<serde_json::Value> {
    if all.is_empty() {
        return None;
    }
    match index {
        MatchIndex::First => Some(all.remove(0)),
        MatchIndex::Last => all.pop(),
        MatchIndex::Random => {
            let i = rng.random_range(0..all.len());
            Some(all.swap_remove(i))
        }
        MatchIndex::All => Some(serde_json::Value::Array(all)),
    }
}

/// Evaluate an XPath 1.0 expression against an XML document.
pub fn xpath_eval(xml: &str, expression: &str) -> Result<Option<String>, String> {
    let package = sxd_document::parser::parse(xml).map_err(|e| format!("XML parse: {e}"))?;
    let doc = package.as_document();
    let factory = sxd_xpath::Factory::new();
    let xpath = factory
        .build(expression)
        .map_err(|e| format!("XPath build: {e}"))?
        .ok_or_else(|| "empty XPath".to_string())?;
    let context = sxd_xpath::Context::new();
    let value = xpath
        .evaluate(&context, doc.root())
        .map_err(|e| format!("XPath eval: {e}"))?;
    Ok(match value {
        sxd_xpath::Value::Nodeset(ns) => ns
            .document_order_first()
            .map(|n| n.string_value().trim().to_string()),
        sxd_xpath::Value::String(s) => {
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        sxd_xpath::Value::Number(n) => Some(n.to_string()),
        sxd_xpath::Value::Boolean(b) => Some(b.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use rand::SeedableRng;

    fn response(body: &str, headers: &[(&str, &str)]) -> ProtocolResponse {
        ProtocolResponse {
            body: Bytes::from(body.to_string()),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn rng() -> rand::rngs::SmallRng {
        rand::rngs::SmallRng::seed_from_u64(42)
    }

    fn compile(yaml: &str) -> CompiledExtractor {
        let spec: Extractor = serde_yaml::from_str(yaml).expect("spec");
        CompiledExtractor::compile(&spec).expect("compile")
    }

    // --- classic extractors (back-compat) ---

    #[test]
    fn jsonpath_keeps_types() {
        let ex = compile(r#"{ type: jsonpath, name: id, expression: "$.items[0].id" }"#);
        let r = response(r#"{"items":[{"id":42},{"id":43}]}"#, &[]);
        assert_eq!(ex.extract(&r, &mut rng()).unwrap(), serde_json::json!(42));
    }

    #[test]
    fn jsonpath_all_matches() {
        let ex =
            compile(r#"{ type: jsonpath, name: ids, expression: "$.items[*].id", index: all }"#);
        let r = response(r#"{"items":[{"id":1},{"id":2}]}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!([1, 2])
        );
    }

    #[test]
    fn regex_groups_and_index() {
        let r = response("a=1 a=2 a=3", &[]);
        let first = compile(r#"{ type: regex, name: x, expression: "a=(\\d)" }"#);
        assert_eq!(
            first.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("1")
        );
        let last = compile(r#"{ type: regex, name: x, expression: "a=(\\d)", index: last }"#);
        assert_eq!(
            last.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("3")
        );
    }

    #[test]
    fn regex_default_on_no_match() {
        let ex = compile(r#"{ type: regex, name: x, expression: "z=(\\d)", default: fallback }"#);
        let r = response("nothing here", &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("fallback")
        );
    }

    #[test]
    fn no_match_no_default_errors() {
        let ex = compile(r#"{ type: regex, name: x, expression: "z=(\\d)" }"#);
        let r = response("nothing", &[]);
        assert!(matches!(
            ex.extract(&r, &mut rng()),
            Err(ExtractError::NoMatch { .. })
        ));
    }

    #[test]
    fn xpath_extraction() {
        let ex = compile(r#"{ type: xpath, name: n, expression: "//item[@id='2']/name" }"#);
        let r = response(
            r#"<catalog><item id="1"><name>alpha</name></item><item id="2"><name>beta</name></item></catalog>"#,
            &[],
        );
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("beta")
        );
    }

    #[test]
    fn css_selector_attribute() {
        let ex = compile(
            r#"{ type: css, name: csrf, expression: "input[name=csrf]", attribute: value }"#,
        );
        let r = response(
            r#"<html><body><form><input name="csrf" value="tok-1"></form></body></html>"#,
            &[],
        );
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("tok-1")
        );
    }

    #[test]
    fn css_selector_text() {
        let ex = compile(r#"{ type: css, name: t, expression: "h1" }"#);
        let r = response("<html><h1>Hello</h1></html>", &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("Hello")
        );
    }

    #[test]
    fn boundary_extraction() {
        let ex = compile(r#"{ type: boundary, name: b, left: "token=\"", right: "\"" }"#);
        let r = response(r#"<a token="abc123">x</a>"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("abc123")
        );
    }

    #[test]
    fn header_extraction() {
        let ex = compile(r#"{ type: header, name: loc, header: Location }"#);
        let r = response("", &[("location", "/next")]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("/next")
        );
    }

    // --- fused chains ---

    #[test]
    fn chain_jmespath_filter() {
        let ex = compile(r#"{ chain: cheapest, jmespath: "items[?price > `10`] | [0].name" }"#);
        let r = response(
            r#"{"items":[{"name":"alpha","price":9},{"name":"beta","price":19}]}"#,
            &[],
        );
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("beta")
        );
    }

    #[test]
    fn chain_jmespath_numeric_keeps_type() {
        let ex = compile(r#"{ chain: total, jmespath: "count" }"#);
        let r = response(r#"{"count":42}"#, &[]);
        assert_eq!(ex.extract(&r, &mut rng()).unwrap(), serde_json::json!(42));
    }

    #[test]
    fn chain_jmespath_array_index() {
        // A JMESPath returning an array honours `index`.
        let ex = compile(r#"{ chain: last_id, jmespath: "items[].id", index: last }"#);
        let r = response(r#"{"items":[{"id":1},{"id":2},{"id":3}]}"#, &[]);
        assert_eq!(ex.extract(&r, &mut rng()).unwrap(), serde_json::json!(3));
    }

    #[test]
    fn chain_coerce_int_and_bounds() {
        let ex =
            compile(r#"{ chain: qty, jsonpath: "$.qty", as: int, check: { min: 1, max: 100 } }"#);
        let r = response(r#"{"qty":"7"}"#, &[]);
        assert_eq!(ex.extract(&r, &mut rng()).unwrap(), serde_json::json!(7));

        // Out of bounds -> CheckFailed.
        let r2 = response(r#"{"qty":"999"}"#, &[]);
        assert!(matches!(
            ex.extract(&r2, &mut rng()),
            Err(ExtractError::CheckFailed { .. })
        ));
    }

    #[test]
    fn chain_transform_pipeline() {
        let ex = compile(
            r#"{ chain: auth, header: X-Token, transform: [trim, { prepend: "Bearer " }] }"#,
        );
        let r = response("", &[("x-token", "  abc  ")]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("Bearer abc")
        );
    }

    #[test]
    fn chain_transform_lowercase_replace() {
        let ex = compile(
            r#"{ chain: slug, jsonpath: "$.name", transform: [lowercase, { replace: [" ", "-"] }] }"#,
        );
        let r = response(r#"{"name":"Hello World"}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("hello-world")
        );
    }

    #[test]
    fn chain_base64_roundtrip() {
        let enc = compile(r#"{ chain: e, jsonpath: "$.v", transform: [base64_encode] }"#);
        let r = response(r#"{"v":"hi"}"#, &[]);
        assert_eq!(
            enc.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("aGk=")
        );

        let dec = compile(r#"{ chain: d, jsonpath: "$.v", transform: [base64_decode] }"#);
        let r2 = response(r#"{"v":"aGk="}"#, &[]);
        assert_eq!(
            dec.extract(&r2, &mut rng()).unwrap(),
            serde_json::json!("hi")
        );
    }

    #[test]
    fn chain_url_encode() {
        let ex = compile(r#"{ chain: q, jsonpath: "$.q", transform: [url_encode] }"#);
        let r = response(r#"{"q":"a b&c"}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("a%20b%26c")
        );
    }

    #[test]
    fn chain_check_oneof() {
        let ex = compile(
            r#"{ chain: status, jsonpath: "$.status", check: { one_of: [pending, shipped] } }"#,
        );
        let ok = response(r#"{"status":"pending"}"#, &[]);
        assert!(ex.extract(&ok, &mut rng()).is_ok());
        let bad = response(r#"{"status":"cancelled"}"#, &[]);
        assert!(matches!(
            ex.extract(&bad, &mut rng()),
            Err(ExtractError::CheckFailed { .. })
        ));
    }

    #[test]
    fn chain_default_when_no_match() {
        let ex = compile(r#"{ chain: id, jsonpath: "$.missing", default: "none" }"#);
        let r = response(r#"{"other":1}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("none")
        );
    }

    #[test]
    fn chain_no_match_no_default_errors() {
        let ex = compile(r#"{ chain: id, jsonpath: "$.missing" }"#);
        let r = response(r#"{"other":1}"#, &[]);
        assert!(matches!(
            ex.extract(&r, &mut rng()),
            Err(ExtractError::NoMatch { .. })
        ));
    }

    #[test]
    fn chain_check_failure_action_propagates() {
        let ex = compile(
            r#"{ chain: id, jsonpath: "$.id", check: { equals: 1, on_failure: abort_iteration } }"#,
        );
        let r = response(r#"{"id":2}"#, &[]);
        match ex.extract(&r, &mut rng()) {
            Err(ExtractError::CheckFailed { on_failure, .. }) => {
                assert_eq!(on_failure, loadr_config::FailureAction::AbortIteration);
            }
            other => panic!("expected CheckFailed, got {other:?}"),
        }
    }

    #[test]
    fn chain_substring_transform() {
        let ex = compile(r#"{ chain: s, jsonpath: "$.v", transform: [{ substring: [0, 3] }] }"#);
        let r = response(r#"{"v":"abcdef"}"#, &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!("abc")
        );
    }

    #[test]
    fn chain_regex_with_coerce_float() {
        let ex = compile(r#"{ chain: price, regex: "price=([0-9.]+)", as: float }"#);
        let r = response("the price=19.95 today", &[]);
        assert_eq!(
            ex.extract(&r, &mut rng()).unwrap(),
            serde_json::json!(19.95)
        );
    }
}
