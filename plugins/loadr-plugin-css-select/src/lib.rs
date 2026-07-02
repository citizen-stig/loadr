//! WASM extractor plugin `css-select`: pull text or an attribute value out of
//! an HTML body using a CSS selector.
//!
//! Config: `{"selector": ".price", "attr": "data-x"}`.
//! - `selector` (required): a CSS selector. Supports compound selectors
//!   (tag, `.class`, `#id`, `[attr]`, `[attr=val]`, `*`) joined by descendant
//!   combinators (whitespace).
//! - `attr` (optional): when set, return that attribute's value from the first
//!   matching element; otherwise return the element's concatenated text.
//!
//! The HTML parser and selector engine are hand-rolled so the crate builds to
//! `wasm32-wasip2` with only pure-Rust dependencies. The selection logic lives
//! in plain functions (no WIT types) so it can be unit-tested on the host.

wit_bindgen::generate!({
    path: "../../crates/loadr-plugin-api/wit",
    world: "loadr-plugin",
});

use exports::loadr::plugin::extractor::Guest as Extractor;
use exports::loadr::plugin::meta::{Guest as Meta, Info};

#[derive(serde::Deserialize)]
struct Config {
    selector: String,
    #[serde(default)]
    attr: Option<String>,
}

struct Plugin;

impl Meta for Plugin {
    fn describe() -> Info {
        Info {
            name: "css-select".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            kind: "extractor".to_string(),
            description: "Extract text or an attribute from an HTML body by CSS selector"
                .to_string(),
        }
    }
}

impl Extractor for Plugin {
    fn extract(body: Vec<u8>, _headers: Vec<(String, String)>, config: String) -> Option<String> {
        let config: Config = serde_json::from_str(&config).ok()?;
        let html = String::from_utf8_lossy(&body);
        let attr = config.attr.as_deref().filter(|a| !a.is_empty());
        css_select(&html, &config.selector, attr)
    }
}

export!(Plugin);

// ---------------------------------------------------------------------------
// Pure logic below (no WIT types) — unit-tested on the host.
// ---------------------------------------------------------------------------

/// A parsed DOM node held in an arena (indices into `Vec<Node>`).
#[derive(Debug)]
enum Node {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<usize>,
    },
    Text(String),
}

/// Void elements never have children / a closing tag.
fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.'
}

/// Decode the handful of HTML entities that matter for text/attr extraction.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '&' {
            // find terminating ';' within a small window
            if let Some(semi) = bytes[i + 1..]
                .iter()
                .take(10)
                .position(|&c| c == ';')
                .map(|p| i + 1 + p)
            {
                let entity: String = bytes[i + 1..semi].iter().collect();
                let replacement = match entity.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{00a0}'),
                    _ => {
                        if let Some(rest) = entity.strip_prefix('#') {
                            let code = if let Some(hex) = rest.strip_prefix(['x', 'X']) {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                rest.parse::<u32>().ok()
                            };
                            code.and_then(char::from_u32)
                        } else {
                            None
                        }
                    }
                };
                if let Some(ch) = replacement {
                    out.push(ch);
                    i = semi + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Parse an HTML document into an arena of nodes, returning the arena plus the
/// list of root node indices in document order.
fn parse_html(html: &str) -> (Vec<Node>, Vec<usize>) {
    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut arena: Vec<Node> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    // stack of open element indices
    let mut stack: Vec<usize> = Vec::new();

    let push_node =
        |arena: &mut Vec<Node>, roots: &mut Vec<usize>, stack: &Vec<usize>, node: Node| -> usize {
            let idx = arena.len();
            arena.push(node);
            if let Some(&parent) = stack.last() {
                if let Node::Element { children, .. } = &mut arena[parent] {
                    children.push(idx);
                }
            } else {
                roots.push(idx);
            }
            idx
        };

    let mut i = 0;
    while i < len {
        if chars[i] == '<' {
            // Comment
            if starts_with(&chars, i, "<!--") {
                if let Some(end) = find_sub(&chars, i + 4, "-->") {
                    i = end + 3;
                } else {
                    i = len;
                }
                continue;
            }
            // Doctype / declaration / processing instruction
            if i + 1 < len && (chars[i + 1] == '!' || chars[i + 1] == '?') {
                if let Some(gt) = chars[i..].iter().position(|&c| c == '>') {
                    i += gt + 1;
                } else {
                    i = len;
                }
                continue;
            }
            // End tag
            if i + 1 < len && chars[i + 1] == '/' {
                let mut j = i + 2;
                let start = j;
                while j < len && is_name_char(chars[j]) {
                    j += 1;
                }
                let name: String = chars[start..j].iter().collect::<String>().to_lowercase();
                // advance past '>'
                while j < len && chars[j] != '>' {
                    j += 1;
                }
                i = if j < len { j + 1 } else { len };
                // pop until matching tag (lenient)
                if let Some(pos) = stack.iter().rposition(
                    |&idx| matches!(&arena[idx], Node::Element { tag, .. } if *tag == name),
                ) {
                    stack.truncate(pos);
                }
                continue;
            }
            // Start tag
            let mut j = i + 1;
            let start = j;
            while j < len && is_name_char(chars[j]) {
                j += 1;
            }
            if j == start {
                // Not a real tag ("<" followed by non-name); treat as text.
                let text: String = chars[i..i + 1].iter().collect();
                push_node(&mut arena, &mut roots, &stack, Node::Text(text));
                i += 1;
                continue;
            }
            let tag: String = chars[start..j].iter().collect::<String>().to_lowercase();
            let (attrs, mut after, self_closing) = parse_attrs(&chars, j);
            if after < len && chars[after] == '>' {
                after += 1;
            }
            i = after;

            let idx = push_node(
                &mut arena,
                &mut roots,
                &stack,
                Node::Element {
                    tag: tag.clone(),
                    attrs,
                    children: Vec::new(),
                },
            );

            if self_closing || is_void(&tag) {
                // no children
            } else if tag == "script" || tag == "style" {
                // Raw text element: consume until matching close tag.
                let close = format!("</{tag}");
                let end = find_sub_ci(&chars, i, &close).unwrap_or(len);
                let raw: String = chars[i..end].iter().collect();
                if !raw.is_empty() {
                    let child = arena.len();
                    arena.push(Node::Text(raw));
                    if let Node::Element { children, .. } = &mut arena[idx] {
                        children.push(child);
                    }
                }
                // skip to end of the close tag
                let mut k = end;
                while k < len && chars[k] != '>' {
                    k += 1;
                }
                i = if k < len { k + 1 } else { len };
            } else {
                stack.push(idx);
            }
        } else {
            // Text run until next '<'
            let start = i;
            while i < len && chars[i] != '<' {
                i += 1;
            }
            let raw: String = chars[start..i].iter().collect();
            let text = decode_entities(&raw);
            push_node(&mut arena, &mut roots, &stack, Node::Text(text));
        }
    }

    (arena, roots)
}

/// Parse attributes starting at `start` (just after the tag name). Returns the
/// attribute list, the index positioned at `>` (or `/`), and whether the tag is
/// self-closing (`/>`).
fn parse_attrs(chars: &[char], start: usize) -> (Vec<(String, String)>, usize, bool) {
    let len = chars.len();
    let mut i = start;
    let mut attrs = Vec::new();
    let mut self_closing = false;
    loop {
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len || chars[i] == '>' {
            break;
        }
        if chars[i] == '/' {
            self_closing = true;
            i += 1;
            continue;
        }
        // attribute name
        let nstart = i;
        while i < len
            && !chars[i].is_whitespace()
            && chars[i] != '='
            && chars[i] != '>'
            && chars[i] != '/'
        {
            i += 1;
        }
        let name: String = chars[nstart..i].iter().collect::<String>().to_lowercase();
        // optional value
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < len && chars[i] == '=' {
            i += 1;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            if i < len && (chars[i] == '"' || chars[i] == '\'') {
                let quote = chars[i];
                i += 1;
                let vstart = i;
                while i < len && chars[i] != quote {
                    i += 1;
                }
                value = chars[vstart..i].iter().collect();
                if i < len {
                    i += 1; // closing quote
                }
            } else {
                let vstart = i;
                while i < len && !chars[i].is_whitespace() && chars[i] != '>' && chars[i] != '/' {
                    i += 1;
                }
                value = chars[vstart..i].iter().collect();
            }
        }
        if !name.is_empty() {
            attrs.push((name, decode_entities(&value)));
        }
    }
    (attrs, i, self_closing)
}

fn starts_with(chars: &[char], at: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    if at + p.len() > chars.len() {
        return false;
    }
    chars[at..at + p.len()] == p[..]
}

fn find_sub(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.chars().collect();
    if p.is_empty() || from > chars.len() {
        return None;
    }
    let mut i = from;
    while i + p.len() <= chars.len() {
        if chars[i..i + p.len()] == p[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Case-insensitive substring search over `chars`.
fn find_sub_ci(chars: &[char], from: usize, pat: &str) -> Option<usize> {
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    if p.is_empty() || from > chars.len() {
        return None;
    }
    let mut i = from;
    while i + p.len() <= chars.len() {
        let matches =
            (0..p.len()).all(|k| chars[i + k].to_lowercase().next() == p[k].to_lowercase().next());
        if matches {
            return Some(i);
        }
        i += 1;
    }
    None
}

// --- Selector model -------------------------------------------------------

#[derive(Debug, PartialEq)]
struct AttrCond {
    name: String,
    value: Option<String>,
}

#[derive(Debug, PartialEq, Default)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<AttrCond>,
}

/// A full selector is a descendant chain of compound selectors.
type Selector = Vec<Compound>;

/// Parse a CSS selector string into a descendant chain of compound selectors.
/// Returns `None` on empty / malformed input.
fn parse_selector(input: &str) -> Option<Selector> {
    let chars: Vec<char> = input.trim().chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut chain: Selector = Vec::new();
    let mut i = 0;
    let len = chars.len();
    while i < len {
        // skip combinator whitespace (only descendant combinator supported)
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }
        let mut c = Compound::default();
        let mut saw_something = false;
        while i < len && !chars[i].is_whitespace() {
            match chars[i] {
                '.' => {
                    i += 1;
                    let start = i;
                    while i < len
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                    {
                        i += 1;
                    }
                    if i == start {
                        return None;
                    }
                    c.classes.push(chars[start..i].iter().collect());
                    saw_something = true;
                }
                '#' => {
                    i += 1;
                    let start = i;
                    while i < len
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                    {
                        i += 1;
                    }
                    if i == start {
                        return None;
                    }
                    c.id = Some(chars[start..i].iter().collect());
                    saw_something = true;
                }
                '[' => {
                    i += 1;
                    let nstart = i;
                    while i < len && chars[i] != '=' && chars[i] != ']' {
                        i += 1;
                    }
                    let name: String = chars[nstart..i]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_lowercase();
                    if name.is_empty() {
                        return None;
                    }
                    let mut value = None;
                    if i < len && chars[i] == '=' {
                        i += 1;
                        // optional quote
                        let quote = if i < len && (chars[i] == '"' || chars[i] == '\'') {
                            let q = chars[i];
                            i += 1;
                            Some(q)
                        } else {
                            None
                        };
                        let vstart = i;
                        match quote {
                            Some(q) => {
                                while i < len && chars[i] != q {
                                    i += 1;
                                }
                                value = Some(chars[vstart..i].iter().collect());
                                if i < len {
                                    i += 1;
                                }
                            }
                            None => {
                                while i < len && chars[i] != ']' {
                                    i += 1;
                                }
                                value = Some(
                                    chars[vstart..i]
                                        .iter()
                                        .collect::<String>()
                                        .trim()
                                        .to_string(),
                                );
                            }
                        }
                    }
                    if i < len && chars[i] == ']' {
                        i += 1;
                    } else {
                        return None;
                    }
                    c.attrs.push(AttrCond { name, value });
                    saw_something = true;
                }
                '*' => {
                    i += 1;
                    c.tag = None;
                    saw_something = true;
                }
                ch if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' => {
                    let start = i;
                    while i < len
                        && (chars[i].is_ascii_alphanumeric() || chars[i] == '-' || chars[i] == '_')
                    {
                        i += 1;
                    }
                    c.tag = Some(chars[start..i].iter().collect::<String>().to_lowercase());
                    saw_something = true;
                }
                _ => return None,
            }
        }
        if !saw_something {
            return None;
        }
        chain.push(c);
    }
    if chain.is_empty() {
        None
    } else {
        Some(chain)
    }
}

fn get_attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let name = name.to_lowercase();
    attrs
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.as_str())
}

/// Does a single element node satisfy one compound selector?
fn matches_compound(node: &Node, c: &Compound) -> bool {
    let (tag, attrs) = match node {
        Node::Element { tag, attrs, .. } => (tag, attrs),
        Node::Text(_) => return false,
    };
    if let Some(want) = &c.tag {
        if want != tag {
            return false;
        }
    }
    if let Some(want_id) = &c.id {
        match get_attr(attrs, "id") {
            Some(id) if id == want_id => {}
            _ => return false,
        }
    }
    if !c.classes.is_empty() {
        let classes = get_attr(attrs, "class").unwrap_or("");
        let present: Vec<&str> = classes.split_whitespace().collect();
        if !c
            .classes
            .iter()
            .all(|want| present.iter().any(|p| p == want))
        {
            return false;
        }
    }
    for cond in &c.attrs {
        match get_attr(attrs, &cond.name) {
            Some(v) => {
                if let Some(want) = &cond.value {
                    if v != want {
                        return false;
                    }
                }
            }
            None => return false,
        }
    }
    true
}

/// Collect and trim the concatenated text content of a subtree.
fn text_content(arena: &[Node], idx: usize, out: &mut String) {
    match &arena[idx] {
        Node::Text(t) => out.push_str(t),
        Node::Element { children, .. } => {
            for &child in children {
                text_content(arena, child, out);
            }
        }
    }
}

/// Walk the arena in document order; for each element, check whether the full
/// descendant selector chain matches ending at that element. Returns the index
/// of the first matching element.
fn find_first_match(arena: &[Node], roots: &[usize], selector: &Selector) -> Option<usize> {
    fn visit(
        arena: &[Node],
        idx: usize,
        ancestors: &mut Vec<usize>,
        selector: &Selector,
        found: &mut Option<usize>,
    ) {
        if found.is_some() {
            return;
        }
        if matches!(arena[idx], Node::Element { .. })
            && matches_chain(arena, idx, ancestors, selector)
        {
            *found = Some(idx);
            return;
        }
        if let Node::Element { children, .. } = &arena[idx] {
            ancestors.push(idx);
            for &child in children {
                visit(arena, child, ancestors, selector, found);
                if found.is_some() {
                    break;
                }
            }
            ancestors.pop();
        }
    }

    let mut found = None;
    let mut ancestors = Vec::new();
    for &root in roots {
        visit(arena, root, &mut ancestors, selector, &mut found);
        if found.is_some() {
            break;
        }
    }
    found
}

/// The last compound must match `idx`; preceding compounds must match distinct
/// ancestors in order (descendant combinator semantics).
fn matches_chain(arena: &[Node], idx: usize, ancestors: &[usize], selector: &Selector) -> bool {
    let last = selector.len() - 1;
    if !matches_compound(&arena[idx], &selector[last]) {
        return false;
    }
    if last == 0 {
        return true;
    }
    // Greedily match remaining compounds (right-to-left) against ancestors
    // (closest first). Any ancestor may match; order must be preserved.
    let mut sel_i = last; // next compound to satisfy is sel_i - 1
    for &anc in ancestors.iter().rev() {
        if sel_i == 0 {
            break;
        }
        if matches_compound(&arena[anc], &selector[sel_i - 1]) {
            sel_i -= 1;
        }
    }
    sel_i == 0
}

/// Top-level: parse HTML, parse selector, find first match, return text or attr.
fn css_select(html: &str, selector: &str, attr: Option<&str>) -> Option<String> {
    let selector = parse_selector(selector)?;
    let (arena, roots) = parse_html(html);
    let idx = find_first_match(&arena, &roots, &selector)?;
    match attr {
        Some(a) => match &arena[idx] {
            Node::Element { attrs, .. } => get_attr(attrs, a).map(|s| s.to_string()),
            _ => None,
        },
        None => {
            let mut out = String::new();
            text_content(&arena, idx, &mut out);
            Some(out.trim().to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_selectors() {
        let s = parse_selector(".price").unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].classes, vec!["price".to_string()]);

        let s = parse_selector("#main").unwrap();
        assert_eq!(s[0].id, Some("main".to_string()));

        let s = parse_selector("div").unwrap();
        assert_eq!(s[0].tag, Some("div".to_string()));

        let s = parse_selector("span.a.b#id").unwrap();
        assert_eq!(s[0].tag, Some("span".to_string()));
        assert_eq!(s[0].id, Some("id".to_string()));
        assert_eq!(s[0].classes, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_attr_selectors() {
        let s = parse_selector("[data-x]").unwrap();
        assert_eq!(s[0].attrs[0].name, "data-x");
        assert_eq!(s[0].attrs[0].value, None);

        let s = parse_selector("a[href=\"/foo\"]").unwrap();
        assert_eq!(s[0].tag, Some("a".to_string()));
        assert_eq!(s[0].attrs[0].name, "href");
        assert_eq!(s[0].attrs[0].value, Some("/foo".to_string()));

        let s = parse_selector("input[type=text]").unwrap();
        assert_eq!(s[0].attrs[0].value, Some("text".to_string()));
    }

    #[test]
    fn parse_descendant_chain() {
        let s = parse_selector("div.card span.price").unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].tag, Some("div".to_string()));
        assert_eq!(s[1].classes, vec!["price".to_string()]);
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_selector("").is_none());
        assert!(parse_selector("   ").is_none());
        assert!(parse_selector(".").is_none());
        assert!(parse_selector("#").is_none());
        assert!(parse_selector("[unclosed").is_none());
    }

    #[test]
    fn extract_text_by_class() {
        let html = r#"<div><span class="price">$19.99</span></div>"#;
        assert_eq!(css_select(html, ".price", None), Some("$19.99".to_string()));
    }

    #[test]
    fn extract_attr() {
        let html = r#"<p class="price" data-x="hidden-value">$5</p>"#;
        assert_eq!(
            css_select(html, ".price", Some("data-x")),
            Some("hidden-value".to_string())
        );
    }

    #[test]
    fn extract_by_id_and_tag() {
        let html = r#"<div id="main">Hello <b>World</b></div>"#;
        assert_eq!(
            css_select(html, "#main", None),
            Some("Hello World".to_string())
        );
        assert_eq!(
            css_select(html, "div#main b", None),
            Some("World".to_string())
        );
    }

    #[test]
    fn extract_nested_text_concatenated_and_trimmed() {
        let html = "<article>  <h1>Title</h1>  <p>Body text</p>  </article>";
        assert_eq!(
            css_select(html, "article", None),
            Some("Title  Body text".to_string())
        );
    }

    #[test]
    fn descendant_matching() {
        let html = r#"
            <div class="card">
                <span class="price">wrong</span>
            </div>
            <div class="featured">
                <span class="price">right</span>
            </div>
        "#;
        // first match in document order
        assert_eq!(
            css_select(html, "span.price", None),
            Some("wrong".to_string())
        );
        // scoped to featured card
        assert_eq!(
            css_select(html, "div.featured span.price", None),
            Some("right".to_string())
        );
    }

    #[test]
    fn attr_value_selector_match() {
        let html = r#"<a href="/a">one</a><a href="/b">two</a>"#;
        assert_eq!(
            css_select(html, r#"a[href="/b"]"#, None),
            Some("two".to_string())
        );
    }

    #[test]
    fn entities_decoded() {
        let html = "<p class=\"m\">A &amp; B &lt;ok&gt; &#36;9</p>";
        assert_eq!(
            css_select(html, ".m", None),
            Some("A & B <ok> $9".to_string())
        );
    }

    #[test]
    fn void_elements_and_attrs() {
        let html = r#"<form><input type="text" name="q" value="hi"><label>Search</label></form>"#;
        assert_eq!(
            css_select(html, "input[name=q]", Some("value")),
            Some("hi".to_string())
        );
        // input is void, so label is a sibling under form, not a child of input
        assert_eq!(
            css_select(html, "form label", None),
            Some("Search".to_string())
        );
    }

    #[test]
    fn no_match_returns_none() {
        let html = "<div>nothing here</div>";
        assert_eq!(css_select(html, ".missing", None), None);
        assert_eq!(css_select(html, ".missing", Some("data-x")), None);
    }

    #[test]
    fn missing_attr_returns_none() {
        let html = r#"<span class="price">$1</span>"#;
        assert_eq!(css_select(html, ".price", Some("data-x")), None);
    }

    #[test]
    fn script_content_ignored_for_element_matching() {
        let html = r#"<script>var x = "<span class='price'>fake</span>";</script><span class="price">real</span>"#;
        assert_eq!(
            css_select(html, "span.price", None),
            Some("real".to_string())
        );
    }

    #[test]
    fn comments_skipped() {
        let html =
            r#"<!-- <span class="price">commented</span> --><span class="price">live</span>"#;
        assert_eq!(css_select(html, ".price", None), Some("live".to_string()));
    }

    #[test]
    fn wildcard_selector() {
        let html = "<body><h1>Head</h1></body>";
        assert_eq!(css_select(html, "*", None), Some("Head".to_string()));
    }
}
