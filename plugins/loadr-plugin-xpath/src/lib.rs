//! WASM extractor plugin `xpath`: pull the first matching node's text (or an
//! attribute value) out of an XML body using an XPath-like expression.
//!
//! Config: `{"xpath": "//id/text()"}`.
//! - `xpath` (required): an XPath expression. A pragmatic, pure-Rust subset is
//!   supported (see below) so the crate builds to `wasm32-wasip2` with no C or
//!   networking dependencies — full XPath engines (sxd-xpath, libxml) do not
//!   compile cleanly to that target, so the walker is hand-rolled.
//!
//! Supported XPath subset:
//! - Absolute (`/root/child`), descendant (`//id`), and relative paths.
//! - The `//` combinator anywhere in the path (descendant-or-self axis).
//! - Name tests (`id`, namespaced `ns:id`, matched by full or local name) and
//!   the `*` wildcard.
//! - A trailing `text()` step (element string value) or `@attr` step
//!   (attribute value).
//! - Positional/other predicates (`[...]`) are tolerated but ignored.
//!
//! The XML parser and evaluator live in plain functions (no WIT types) so they
//! can be unit-tested on the host; only the thin `Extractor`/`Meta` glue below
//! touches the generated bindings.

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-plugin",
});

use exports::loadr::plugin::extractor::Guest as Extractor;
use exports::loadr::plugin::meta::{Guest as Meta, Info};

#[derive(serde::Deserialize)]
struct Config {
    xpath: String,
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "xpath".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "extractor".to_string(),
            description: "Extract the first node text or attribute from an XML body by XPath"
                .to_string(),
        }
    }
}

impl Extractor for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let config: Config = serde_json::from_str(&config).ok()?;
        let text = String::from_utf8_lossy(&body);
        extract_xpath(&text, &config.xpath)
    }
}

export!(Plugin);

// ---------------------------------------------------------------------------
// Pure logic below (no WIT types) — unit-tested on the host.
// ---------------------------------------------------------------------------

/// A parsed XML element. Text runs and nested elements are interleaved in
/// document order in `content` so mixed content keeps its ordering.
#[derive(Debug, Clone)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    content: Vec<Content>,
}

/// One piece of an element's content, in document order.
#[derive(Debug, Clone)]
enum Content {
    Text(String),
    Child(Element),
}

impl Element {
    /// Concatenation of this element's *direct* text runs (the `text()` value).
    fn direct_text(&self) -> String {
        let mut s = String::new();
        for c in &self.content {
            if let Content::Text(t) = c {
                s.push_str(t);
            }
        }
        s
    }

    /// Direct child elements, in document order.
    fn child_elems(&self) -> impl Iterator<Item = &Element> {
        self.content.iter().filter_map(|c| match c {
            Content::Child(e) => Some(e),
            Content::Text(_) => None,
        })
    }

    fn has_child_elem(&self) -> bool {
        self.child_elems().next().is_some()
    }
}

/// XPath axis for a single step.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    Child,
    Descendant,
}

/// A node test for an element step.
#[derive(Debug, Clone, PartialEq)]
enum Test {
    Name(String),
    Wildcard,
}

/// One element-selecting step of a path.
#[derive(Debug, Clone)]
struct Step {
    axis: Axis,
    test: Test,
}

/// What to return from the final matched element.
#[derive(Debug, Clone, PartialEq)]
enum Terminal {
    /// Element string value (concatenation of all descendant text).
    Node,
    /// `text()` — the element's direct text.
    Text,
    /// `@name` — an attribute value.
    Attr(String),
}

/// Top-level entry point: parse the XML, parse the XPath, evaluate, return the
/// first match's string value / attribute value. `None` on parse failure or no
/// match.
fn extract_xpath(xml: &str, xpath: &str) -> Option<String> {
    let doc = parse_xml(xml)?;
    let (steps, terminal) = parse_xpath(xpath)?;
    if steps.is_empty() {
        return None;
    }

    let mut ctx: Vec<&Element> = vec![&doc];
    for step in &steps {
        ctx = eval_step(&ctx, step);
        if ctx.is_empty() {
            return None;
        }
    }
    apply_terminal(ctx.first()?, &terminal)
}

// --- XPath parsing ---------------------------------------------------------

fn parse_xpath(xpath: &str) -> Option<(Vec<Step>, Terminal)> {
    let xp = xpath.trim();
    if xp.is_empty() {
        return None;
    }

    // Determine the axis of the first step and strip the leading marker.
    let (mut pending_axis, rest) = if let Some(r) = xp.strip_prefix("//") {
        (Axis::Descendant, r)
    } else if let Some(r) = xp.strip_prefix('/') {
        (Axis::Child, r)
    } else {
        (Axis::Child, xp)
    };

    let mut steps = Vec::new();
    let mut terminal = Terminal::Node;

    for seg in rest.split('/') {
        if seg.is_empty() {
            // An empty segment comes from `//` — the *next* step is a
            // descendant.
            pending_axis = Axis::Descendant;
            continue;
        }

        // Drop any predicate; strip a leading `child::`-style axis prefix we
        // do not model, keeping only the node test.
        let base = seg.split('[').next().unwrap_or(seg).trim();
        if base.is_empty() {
            continue;
        }

        if base == "text()" || base == "node()" {
            terminal = Terminal::Text;
            continue;
        }
        if let Some(attr) = base.strip_prefix('@') {
            terminal = Terminal::Attr(attr.to_string());
            continue;
        }

        let test = if base == "*" {
            Test::Wildcard
        } else {
            Test::Name(base.to_string())
        };
        steps.push(Step {
            axis: pending_axis,
            test,
        });
        pending_axis = Axis::Child;
    }

    Some((steps, terminal))
}

// --- Evaluation ------------------------------------------------------------

fn local_name(name: &str) -> &str {
    match name.rfind(':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

fn test_matches(test: &Test, el: &Element) -> bool {
    match test {
        Test::Wildcard => !el.name.is_empty(),
        Test::Name(n) => el.name == *n || local_name(&el.name) == n.as_str(),
    }
}

fn collect_descendant_or_self<'a>(node: &'a Element, out: &mut Vec<&'a Element>) {
    out.push(node);
    for child in node.child_elems() {
        collect_descendant_or_self(child, out);
    }
}

fn eval_step<'a>(ctx: &[&'a Element], step: &Step) -> Vec<&'a Element> {
    let mut out: Vec<&Element> = Vec::new();
    for &node in ctx {
        match step.axis {
            Axis::Child => {
                for child in node.child_elems() {
                    if test_matches(&step.test, child) {
                        out.push(child);
                    }
                }
            }
            Axis::Descendant => {
                let mut all = Vec::new();
                collect_descendant_or_self(node, &mut all);
                for d in all {
                    if test_matches(&step.test, d) {
                        out.push(d);
                    }
                }
            }
        }
    }
    out
}

/// Concatenation of all descendant text (XPath string-value of an element).
fn string_value(el: &Element) -> String {
    let mut out = String::new();
    for c in &el.content {
        match c {
            Content::Text(t) => out.push_str(t),
            Content::Child(child) => out.push_str(&string_value(child)),
        }
    }
    out
}

fn apply_terminal(el: &Element, terminal: &Terminal) -> Option<String> {
    match terminal {
        Terminal::Text => Some(el.direct_text().trim().to_string()),
        Terminal::Node => Some(string_value(el).trim().to_string()),
        Terminal::Attr(name) => el
            .attrs
            .iter()
            .find(|(k, _)| k == name || local_name(k) == name.as_str())
            .map(|(_, v)| v.clone()),
    }
}

// --- XML parsing -----------------------------------------------------------

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // Find the terminating ';' within a small window.
        let mut j = i + 1;
        while j < chars.len() && chars[j] != ';' && j - i <= 10 {
            j += 1;
        }
        if j < chars.len() && chars[j] == ';' {
            let ent: String = chars[i + 1..j].iter().collect();
            let replacement = match ent.as_str() {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => decode_numeric_entity(&ent),
            };
            if let Some(c) = replacement {
                out.push(c);
                i = j + 1;
                continue;
            }
        }
        // Not a recognised entity — keep the '&' literally.
        out.push('&');
        i += 1;
    }
    out
}

fn decode_numeric_entity(ent: &str) -> Option<char> {
    let rest = ent.strip_prefix('#')?;
    let code = if let Some(hex) = rest.strip_prefix('x').or_else(|| rest.strip_prefix('X')) {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        rest.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

struct Parser {
    s: Vec<char>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn starts_with(&self, pat: &str) -> bool {
        let p: Vec<char> = pat.chars().collect();
        if self.i + p.len() > self.s.len() {
            return false;
        }
        self.s[self.i..self.i + p.len()] == p[..]
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn read_name(&mut self) -> String {
        let mut name = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == '/' || c == '>' || c == '=' {
                break;
            }
            name.push(c);
            self.i += 1;
        }
        name
    }

    fn parse_attrs(&mut self, el: &mut Element) {
        loop {
            self.skip_ws();
            match self.peek() {
                Some('/') | Some('>') | None => break,
                _ => {}
            }
            let name = self.read_name();
            if name.is_empty() {
                // Guard against stalling on an unexpected character.
                self.i += 1;
                continue;
            }
            self.skip_ws();
            let mut value = String::new();
            if self.peek() == Some('=') {
                self.i += 1;
                self.skip_ws();
                if let Some(q) = self.peek() {
                    if q == '"' || q == '\'' {
                        self.i += 1;
                        let mut raw = String::new();
                        while let Some(c) = self.peek() {
                            if c == q {
                                break;
                            }
                            raw.push(c);
                            self.i += 1;
                        }
                        self.i += 1; // consume closing quote
                        value = decode_entities(&raw);
                    }
                }
            }
            el.attrs.push((name, value));
        }
    }

    /// At a `<` that begins a start tag. Returns the fully parsed element,
    /// including its subtree and closing tag.
    fn parse_element(&mut self) -> Element {
        self.i += 1; // consume '<'
        let name = self.read_name();
        let mut el = Element {
            name,
            attrs: Vec::new(),
            content: Vec::new(),
        };
        self.parse_attrs(&mut el);
        self.skip_ws();

        if self.starts_with("/>") {
            self.i += 2;
            return el;
        }
        if self.peek() == Some('>') {
            self.i += 1;
        } else {
            return el; // malformed — bail with what we have
        }

        self.parse_nodes(&mut el);

        if self.starts_with("</") {
            self.i += 2;
            let _closing = self.read_name();
            while let Some(c) = self.peek() {
                self.i += 1;
                if c == '>' {
                    break;
                }
            }
        }
        el
    }

    /// Parse the child nodes of `into` until a closing tag or EOF. Text runs
    /// and child elements are appended to `into.content` in document order.
    fn parse_nodes(&mut self, into: &mut Element) {
        loop {
            if self.i >= self.s.len() {
                break;
            }
            if self.starts_with("</") {
                break;
            }
            if self.starts_with("<!--") {
                self.i += 4;
                while self.i < self.s.len() && !self.starts_with("-->") {
                    self.i += 1;
                }
                if self.starts_with("-->") {
                    self.i += 3;
                }
                continue;
            }
            if self.starts_with("<![CDATA[") {
                self.i += 9;
                let mut raw = String::new();
                while self.i < self.s.len() && !self.starts_with("]]>") {
                    raw.push(self.s[self.i]);
                    self.i += 1;
                }
                if self.starts_with("]]>") {
                    self.i += 3;
                }
                into.content.push(Content::Text(raw));
                continue;
            }
            if self.starts_with("<?") {
                self.i += 2;
                while self.i < self.s.len() && !self.starts_with("?>") {
                    self.i += 1;
                }
                if self.starts_with("?>") {
                    self.i += 2;
                }
                continue;
            }
            if self.starts_with("<!") {
                // DOCTYPE or similar — skip to the next '>'.
                self.i += 2;
                while self.i < self.s.len() && self.peek() != Some('>') {
                    self.i += 1;
                }
                if self.peek() == Some('>') {
                    self.i += 1;
                }
                continue;
            }
            if self.peek() == Some('<') {
                let child = self.parse_element();
                into.content.push(Content::Child(child));
                continue;
            }
            // Text run.
            let mut raw = String::new();
            while let Some(c) = self.peek() {
                if c == '<' {
                    break;
                }
                raw.push(c);
                self.i += 1;
            }
            into.content.push(Content::Text(decode_entities(&raw)));
        }
    }
}

/// Parse an XML document into a synthetic root element whose `children` are the
/// document's top-level elements. Returns `None` if no element is found.
fn parse_xml(input: &str) -> Option<Element> {
    let mut p = Parser {
        s: input.chars().collect(),
        i: 0,
    };
    let mut doc = Element {
        name: String::new(),
        attrs: Vec::new(),
        content: Vec::new(),
    };
    p.parse_nodes(&mut doc);
    if !doc.has_child_elem() {
        return None;
    }
    Some(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendant_text() {
        let xml = r#"<root><id>42</id></root>"#;
        assert_eq!(extract_xpath(xml, "//id/text()").as_deref(), Some("42"));
    }

    #[test]
    fn descendant_element_string_value() {
        let xml = r#"<root><id>42</id></root>"#;
        // Without text(), an element match yields its string value.
        assert_eq!(extract_xpath(xml, "//id").as_deref(), Some("42"));
    }

    #[test]
    fn first_match_wins() {
        let xml = r#"<root><id>1</id><id>2</id></root>"#;
        assert_eq!(extract_xpath(xml, "//id/text()").as_deref(), Some("1"));
    }

    #[test]
    fn absolute_path() {
        let xml = r#"<catalog><book><title>Rust</title></book></catalog>"#;
        assert_eq!(
            extract_xpath(xml, "/catalog/book/title/text()").as_deref(),
            Some("Rust")
        );
    }

    #[test]
    fn relative_path() {
        let xml = r#"<catalog><book><title>Rust</title></book></catalog>"#;
        assert_eq!(
            extract_xpath(xml, "catalog/book/title").as_deref(),
            Some("Rust")
        );
    }

    #[test]
    fn attribute() {
        let xml = r#"<catalog><book id="b1">Rust</book></catalog>"#;
        assert_eq!(extract_xpath(xml, "//book/@id").as_deref(), Some("b1"));
    }

    #[test]
    fn wildcard_step() {
        let xml = r#"<root><wrap><id>99</id></wrap></root>"#;
        assert_eq!(
            extract_xpath(xml, "/root/*/id/text()").as_deref(),
            Some("99")
        );
    }

    #[test]
    fn descendant_in_middle() {
        let xml = r#"<a><b><c><id>7</id></c></b></a>"#;
        assert_eq!(extract_xpath(xml, "/a//id/text()").as_deref(), Some("7"));
    }

    #[test]
    fn namespaced_local_name_match() {
        let xml = r#"<root xmlns:ns="urn:x"><ns:id>abc</ns:id></root>"#;
        // Matched by local name.
        assert_eq!(extract_xpath(xml, "//id/text()").as_deref(), Some("abc"));
        // Matched by full prefixed name too.
        assert_eq!(extract_xpath(xml, "//ns:id/text()").as_deref(), Some("abc"));
    }

    #[test]
    fn entities_decoded() {
        let xml = r#"<root><msg>a &amp; b &lt;c&gt; &#65; &#x42;</msg></root>"#;
        assert_eq!(
            extract_xpath(xml, "//msg/text()").as_deref(),
            Some("a & b <c> A B")
        );
    }

    #[test]
    fn cdata_text() {
        let xml = r#"<root><raw><![CDATA[<hi> & bye]]></raw></root>"#;
        assert_eq!(
            extract_xpath(xml, "//raw/text()").as_deref(),
            Some("<hi> & bye")
        );
    }

    #[test]
    fn self_closing_and_attrs() {
        let xml = r#"<root><item sku="X-1" qty="3"/></root>"#;
        assert_eq!(extract_xpath(xml, "//item/@sku").as_deref(), Some("X-1"));
        assert_eq!(extract_xpath(xml, "//item/@qty").as_deref(), Some("3"));
    }

    #[test]
    fn prolog_and_comments_ignored() {
        let xml = r#"<?xml version="1.0"?><!-- c --><root><id>ok</id></root>"#;
        assert_eq!(extract_xpath(xml, "//id/text()").as_deref(), Some("ok"));
    }

    #[test]
    fn predicate_is_tolerated() {
        let xml = r#"<root><id>1</id><id>2</id></root>"#;
        // Predicate is ignored; still returns the first match.
        assert_eq!(extract_xpath(xml, "//id[1]/text()").as_deref(), Some("1"));
    }

    #[test]
    fn string_value_concatenates_children() {
        let xml = r#"<root><p>Hello <b>brave</b> world</p></root>"#;
        assert_eq!(
            extract_xpath(xml, "//p").as_deref(),
            Some("Hello brave world")
        );
    }

    #[test]
    fn no_match_returns_none() {
        let xml = r#"<root><id>1</id></root>"#;
        assert_eq!(extract_xpath(xml, "//missing/text()"), None);
        assert_eq!(extract_xpath(xml, "//id/@nope"), None);
    }

    #[test]
    fn missing_attr_is_none() {
        let xml = r#"<root><id x="1">v</id></root>"#;
        assert_eq!(extract_xpath(xml, "//id/@y"), None);
    }

    #[test]
    fn invalid_xml_returns_none() {
        assert_eq!(extract_xpath("not xml at all", "//id/text()"), None);
        assert_eq!(extract_xpath("", "//id/text()"), None);
    }

    #[test]
    fn empty_or_root_only_xpath_is_none() {
        let xml = r#"<root><id>1</id></root>"#;
        assert_eq!(extract_xpath(xml, ""), None);
        assert_eq!(extract_xpath(xml, "/"), None);
    }

    #[test]
    fn deep_nested_first_in_document_order() {
        let xml = r#"<root>
            <a><id>alpha</id></a>
            <b><id>beta</id></b>
        </root>"#;
        assert_eq!(extract_xpath(xml, "//id/text()").as_deref(), Some("alpha"));
    }

    #[test]
    fn parse_xpath_shapes() {
        let (steps, term) = parse_xpath("//a/b/text()").unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].axis, Axis::Descendant);
        assert_eq!(steps[0].test, Test::Name("a".to_string()));
        assert_eq!(steps[1].axis, Axis::Child);
        assert_eq!(term, Terminal::Text);

        let (steps, term) = parse_xpath("/x/@id").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].axis, Axis::Child);
        assert_eq!(term, Terminal::Attr("id".to_string()));

        assert!(parse_xpath("   ").is_none());
    }
}
