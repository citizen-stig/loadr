//! `${...}` interpolation templates.
//!
//! Grammar:
//! - `${expr}` — an expression resolved at runtime. Nested `{}` are balanced, so
//!   `${js: ({a:1}).a}` works.
//! - `$${` — escaped literal `${`.
//!
//! Expression namespaces (resolution happens in the engine):
//! - `env.NAME` — process environment
//! - `vars.name` — `variables:` block
//! - `secrets.name` — `secrets:` block (redacted in logs)
//! - `data.source.column` — current data row
//! - `js: <code>` — evaluate JS in the VU's runtime
//! - bare `name` — extracted/iteration variable

use std::fmt;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("unterminated `${{` starting at byte {0}; close it with `}}` or escape it as `$${{`")]
    Unterminated(usize),
    #[error("empty `${{}}` expression at byte {0}")]
    Empty(usize),
    #[error("unresolved template variable `{0}`")]
    Unresolved(String),
}

/// One piece of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Lit(String),
    Expr(String),
}

/// A parsed `${...}` template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub parts: Vec<Part>,
    source: String,
}

impl Template {
    pub fn parse(s: &str) -> Result<Template, TemplateError> {
        let mut parts = Vec::new();
        let mut lit = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() {
                // Escaped `$${` -> literal `${`
                if bytes[i + 1] == b'$' && i + 2 < bytes.len() && bytes[i + 2] == b'{' {
                    lit.push_str("${");
                    i += 3;
                    continue;
                }
                if bytes[i + 1] == b'{' {
                    let start = i;
                    let mut depth = 1usize;
                    let mut j = i + 2;
                    while j < bytes.len() {
                        match bytes[j] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        j += 1;
                    }
                    if depth != 0 {
                        return Err(TemplateError::Unterminated(start));
                    }
                    let expr = s[i + 2..j].trim();
                    if expr.is_empty() {
                        return Err(TemplateError::Empty(start));
                    }
                    if !lit.is_empty() {
                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                    }
                    parts.push(Part::Expr(expr.to_string()));
                    i = j + 1;
                    continue;
                }
            }
            // Advance one UTF-8 character.
            let ch_len = utf8_len(bytes[i]);
            lit.push_str(&s[i..i + ch_len]);
            i += ch_len;
        }
        if !lit.is_empty() {
            parts.push(Part::Lit(lit));
        }
        Ok(Template {
            parts,
            source: s.to_string(),
        })
    }

    /// The original template source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// True when the template contains no expressions.
    pub fn is_literal(&self) -> bool {
        self.parts.iter().all(|p| matches!(p, Part::Lit(_)))
    }

    /// All expressions in the template.
    pub fn expressions(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().filter_map(|p| match p {
            Part::Expr(e) => Some(e.as_str()),
            Part::Lit(_) => None,
        })
    }

    /// Render with a resolver; unresolved expressions are an error.
    pub fn render<F>(&self, mut resolve: F) -> Result<String, TemplateError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut out = String::with_capacity(self.source.len());
        for part in &self.parts {
            match part {
                Part::Lit(l) => out.push_str(l),
                Part::Expr(e) => match resolve(e) {
                    Some(v) => out.push_str(&v),
                    None => return Err(TemplateError::Unresolved(e.clone())),
                },
            }
        }
        Ok(out)
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

fn utf8_len(first_byte: u8) -> usize {
    match first_byte {
        b if b < 0x80 => 1,
        b if b >> 5 == 0b110 => 2,
        b if b >> 4 == 0b1110 => 3,
        _ => 4,
    }
}

/// Walk every string leaf of a JSON value, rendering templates in place.
pub fn render_json_value<F>(
    value: &serde_json::Value,
    resolve: &mut F,
) -> Result<serde_json::Value, TemplateError>
where
    F: FnMut(&str) -> Option<String>,
{
    Ok(match value {
        serde_json::Value::String(s) => {
            let t = Template::parse(s)?;
            if t.is_literal() {
                serde_json::Value::String(s.clone())
            } else if t.parts.len() == 1 {
                // A lone `${expr}` keeps JSON typing where possible: numbers/bools
                // resolved from strings stay strings, but JSON-looking values parse.
                let rendered = t.render(&mut *resolve)?;
                serde_json::from_str(&rendered).unwrap_or(serde_json::Value::String(rendered))
            } else {
                serde_json::Value::String(t.render(&mut *resolve)?)
            }
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|v| render_json_value(v, resolve))
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), render_json_value(v, resolve)?);
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_only() {
        let t = Template::parse("plain text").unwrap();
        assert!(t.is_literal());
        assert_eq!(t.render(|_| None).unwrap(), "plain text");
    }

    #[test]
    fn simple_expression() {
        let t = Template::parse("Bearer ${vars.token}").unwrap();
        assert_eq!(
            t.parts,
            vec![Part::Lit("Bearer ".into()), Part::Expr("vars.token".into())]
        );
        let out = t
            .render(|e| (e == "vars.token").then(|| "abc123".to_string()))
            .unwrap();
        assert_eq!(out, "Bearer abc123");
    }

    #[test]
    fn nested_braces() {
        let t = Template::parse("${js: ({a: {b: 2}}).a.b}").unwrap();
        assert_eq!(t.parts, vec![Part::Expr("js: ({a: {b: 2}}).a.b".into())]);
    }

    #[test]
    fn escaped_dollar() {
        let t = Template::parse("cost: $${notavar}").unwrap();
        assert!(t.is_literal());
        assert_eq!(t.render(|_| None).unwrap(), "cost: ${notavar}");
    }

    #[test]
    fn unterminated_is_error() {
        assert_eq!(
            Template::parse("${oops").unwrap_err(),
            TemplateError::Unterminated(0)
        );
    }

    #[test]
    fn empty_is_error() {
        assert!(matches!(
            Template::parse("a ${ } b").unwrap_err(),
            TemplateError::Empty(_)
        ));
    }

    #[test]
    fn unresolved_is_error() {
        let t = Template::parse("${missing}").unwrap();
        assert_eq!(
            t.render(|_| None).unwrap_err(),
            TemplateError::Unresolved("missing".into())
        );
    }

    #[test]
    fn json_value_rendering_preserves_types() {
        let v = serde_json::json!({
            "id": "${item_id}",
            "label": "item-${item_id}",
            "nested": [ "${count}" ]
        });
        let mut resolve = |e: &str| match e {
            "item_id" => Some("42".to_string()),
            "count" => Some("7".to_string()),
            _ => None,
        };
        let out = render_json_value(&v, &mut resolve).unwrap();
        assert_eq!(out["id"], serde_json::json!(42));
        assert_eq!(out["label"], serde_json::json!("item-42"));
        assert_eq!(out["nested"][0], serde_json::json!(7));
    }

    #[test]
    fn multibyte_literals() {
        let t = Template::parse("héllo ${x} wörld").unwrap();
        let out = t.render(|_| Some("✓".into())).unwrap();
        assert_eq!(out, "héllo ✓ wörld");
    }
}
