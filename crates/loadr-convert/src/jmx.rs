//! JMeter 5.x `.jmx` → loadr [`TestPlan`] conversion.
//!
//! A JMX file is `<jmeterTestPlan><hashTree>...</hashTree></jmeterTestPlan>`,
//! where every element is followed by a sibling `<hashTree>` holding its
//! children. We parse the XML into a small generic tree, then walk those
//! (element, hashTree) pairs.

use std::collections::BTreeSet;

use indexmap::IndexMap;
use loadr_config::{
    Body, BodySpec, Condition, DataMode, DataSource, Dur, ExecutorKind, Extractor, GroupStep,
    MatchIndex, OnEof, PacingSpec, RequestStep, Scenario, Stage, Step, TestPlan, ThinkTimeSpec,
};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::{Conversion, ConversionWarning, ConvertError};

/// Convert a JMeter 5.x `.jmx` document into a loadr test plan.
///
/// Unknown or unsupported elements never fail the conversion; they are
/// reported as [`ConversionWarning`]s instead.
pub fn convert_jmx(xml: &str) -> Result<Conversion, ConvertError> {
    let root = parse_tree(xml)?;
    let jtp = root
        .children
        .iter()
        .find(|n| n.tag == "jmeterTestPlan")
        .ok_or_else(|| ConvertError::NotJmx("missing <jmeterTestPlan> root element".into()))?;
    let top_ht = jtp
        .children
        .iter()
        .find(|n| n.tag == "hashTree")
        .ok_or_else(|| ConvertError::NotJmx("missing top-level <hashTree>".into()))?;

    let mut cv = JmxConverter::default();

    // Locate the TestPlan element and its hashTree of children.
    let top_pairs = pairs(&top_ht.children);
    let (tp, tp_ht) = top_pairs
        .iter()
        .find(|(n, _)| n.tag == "TestPlan")
        .copied()
        .ok_or_else(|| ConvertError::NotJmx("no <TestPlan> element found".into()))?;

    cv.plan.name = non_empty(tp.testname()).map(str::to_string);
    if let Some(udv) = tp.element_prop("TestPlan.user_defined_variables") {
        cv.collect_arguments(udv);
    }

    let plan_children = tp_ht.map(|h| pairs(&h.children)).unwrap_or_default();

    // Pass 1: register variables and CSV data sources anywhere in the tree so
    // `${name}` references can be rewritten when samplers are converted.
    for &(node, ht) in &plan_children {
        cv.prescan(node, ht);
    }

    // Pass 2: convert thread groups and plan-level config elements.
    for &(node, ht) in &plan_children {
        if !node.enabled() {
            cv.warn(node.label(), "element is disabled; skipped");
            continue;
        }
        match node.tag.as_str() {
            "ThreadGroup" | "SetupThreadGroup" | "PostThreadGroup" => {
                cv.convert_thread_group(node, ht);
            }
            "HeaderManager" => {
                let headers = parse_header_manager(node);
                for (k, v) in headers {
                    let v = cv.rewrite(&v);
                    cv.plan.defaults.http.headers.insert(k, v);
                }
            }
            "CookieManager" => cv.convert_cookie_manager(node),
            "CSVDataSet" | "Arguments" => {} // handled in prescan
            "ConstantThroughputTimer" => cv.warn(
                node.label(),
                "constant throughput timer at test-plan level is not supported; \
                 move it into a thread group to become scenario pacing",
            ),
            "ResultCollector" => cv.warn(
                node.label(),
                "listeners are not converted; configure loadr `outputs:` instead",
            ),
            other => cv.warn(
                node.label(),
                format!("unsupported element <{other}> at test-plan level; skipped"),
            ),
        }
    }

    cv.hoist_base_url();

    Ok(Conversion {
        plan: cv.plan,
        warnings: cv.warnings,
    })
}

// ---------------------------------------------------------------------------
// Generic XML tree
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn enabled(&self) -> bool {
        self.attr("enabled") != Some("false")
    }

    fn testname(&self) -> &str {
        self.attr("testname").unwrap_or("")
    }

    /// `testname` when present, otherwise the tag — for warning messages.
    fn label(&self) -> String {
        match non_empty(self.testname()) {
            Some(n) => format!("{} ({})", self.tag, n),
            None => self.tag.clone(),
        }
    }

    /// Value of a direct `stringProp`/`boolProp`/`intProp`/`longProp`/`doubleProp`
    /// child. Handles both the attribute form (`<stringProp name="x">v</stringProp>`)
    /// and the nested form JMeter uses for `doubleProp`
    /// (`<doubleProp><name>x</name><value>v</value></doubleProp>`).
    fn prop(&self, name: &str) -> Option<&str> {
        for c in &self.children {
            if !c.tag.ends_with("Prop") || c.tag == "elementProp" || c.tag == "collectionProp" {
                continue;
            }
            if c.attr("name") == Some(name) {
                return Some(c.text.as_str());
            }
            let nested = c
                .children
                .iter()
                .find(|n| n.tag == "name")
                .map(|n| n.text.trim());
            if nested == Some(name) {
                if let Some(v) = c.children.iter().find(|n| n.tag == "value") {
                    return Some(v.text.as_str());
                }
            }
        }
        None
    }

    fn bool_prop(&self, name: &str) -> Option<bool> {
        self.prop(name).map(|v| v.trim() == "true")
    }

    fn int_prop(&self, name: &str) -> Option<i64> {
        self.prop(name).and_then(|v| v.trim().parse().ok())
    }

    fn element_prop(&self, name: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|c| c.tag == "elementProp" && c.attr("name") == Some(name))
    }

    fn collection_prop(&self, name: &str) -> Option<&Node> {
        self.children
            .iter()
            .find(|c| c.tag == "collectionProp" && c.attr("name") == Some(name))
    }
}

fn parse_tree(xml: &str) -> Result<Node, ConvertError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = false;
    let mut stack: Vec<Node> = vec![Node {
        tag: String::from("(document)"),
        ..Default::default()
    }];

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let node = start_node(&e)?;
                stack.push(node);
            }
            Ok(Event::Empty(e)) => {
                let node = start_node(&e)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    // The root sentinel is always at index 0.
                    if let Some(node) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(node);
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let s = t
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .map_err(|e| ConvertError::Xml(e.to_string()))?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&s);
                }
            }
            Ok(Event::CData(c)) => {
                let s = c.decode().map_err(|e| ConvertError::Xml(e.to_string()))?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&s);
                }
            }
            Ok(Event::GeneralRef(r)) => {
                let resolved = resolve_ref(&r)?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&resolved);
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(ConvertError::Xml(e.to_string())),
        }
    }

    stack
        .into_iter()
        .next()
        .ok_or_else(|| ConvertError::Xml("empty document".into()))
}

fn start_node(e: &quick_xml::events::BytesStart<'_>) -> Result<Node, ConvertError> {
    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut attrs = Vec::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|e| ConvertError::Xml(e.to_string()))?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| ConvertError::Xml(e.to_string()))?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Node {
        tag,
        attrs,
        ..Default::default()
    })
}

fn resolve_ref(r: &quick_xml::events::BytesRef<'_>) -> Result<String, ConvertError> {
    if let Some(ch) = r
        .resolve_char_ref()
        .map_err(|e| ConvertError::Xml(e.to_string()))?
    {
        return Ok(ch.to_string());
    }
    let name = r.decode().map_err(|e| ConvertError::Xml(e.to_string()))?;
    Ok(match name.as_ref() {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        other => format!("&{other};"),
    })
}

/// Pair every element with its following sibling `<hashTree>` (JMX layout).
fn pairs(nodes: &[Node]) -> Vec<(&Node, Option<&Node>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        let n = &nodes[i];
        if n.tag == "hashTree" {
            i += 1;
            continue;
        }
        let ht = nodes.get(i + 1).filter(|h| h.tag == "hashTree");
        out.push((n, ht));
        i += if ht.is_some() { 2 } else { 1 };
    }
    out
}

// ---------------------------------------------------------------------------
// Converter
// ---------------------------------------------------------------------------

#[derive(Default)]
struct JmxConverter {
    plan: TestPlan,
    warnings: Vec<ConversionWarning>,
    /// `${name}` → replacement (`${vars.name}` / `${data.src.name}`).
    rewrites: Vec<(String, String)>,
    /// Functions like `${__threadNum}` we already warned about.
    warned_functions: BTreeSet<String>,
    scenario_names: BTreeSet<String>,
    data_names: BTreeSet<String>,
    /// `scheme://host[:port]` seen on absolute sampler URLs (for hoisting).
    bases: BTreeSet<String>,
}

impl JmxConverter {
    fn warn(&mut self, element: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(ConversionWarning {
            element: element.into(),
            message: message.into(),
        });
    }

    // -- variables / data prescan -------------------------------------------

    /// Recursively register UDV variables and CSV data sets before converting
    /// samplers, so `${name}` rewrites are known up front.
    fn prescan(&mut self, node: &Node, ht: Option<&Node>) {
        if node.enabled() {
            match node.tag.as_str() {
                "Arguments" => self.collect_arguments(node),
                "CSVDataSet" => self.convert_csv_data_set(node),
                _ => {}
            }
        }
        if let Some(ht) = ht {
            for (child, child_ht) in pairs(&ht.children) {
                self.prescan(child, child_ht);
            }
        }
    }

    /// Collect `Argument` entries from an `Arguments` element (UDVs).
    fn collect_arguments(&mut self, node: &Node) {
        let Some(coll) = node.collection_prop("Arguments.arguments") else {
            return;
        };
        for arg in coll.children.iter().filter(|c| c.tag == "elementProp") {
            let name = arg
                .prop("Argument.name")
                .or_else(|| arg.attr("name"))
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let value = arg.prop("Argument.value").unwrap_or("").to_string();
            self.plan
                .variables
                .insert(name.clone(), serde_json::Value::String(value));
            self.rewrites
                .push((format!("${{{name}}}"), format!("${{vars.{name}}}")));
        }
    }

    fn convert_csv_data_set(&mut self, node: &Node) {
        let label = node.label();
        let filename = node.prop("filename").unwrap_or("").trim().to_string();
        if filename.is_empty() {
            self.warn(label, "CSV data set has no filename; skipped");
            return;
        }
        let source = unique_name(&mut self.data_names, slugify(node.testname(), "csv"));
        let variable_names: Vec<String> = node
            .prop("variableNames")
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let ignore_first_line = node.bool_prop("ignoreFirstLine").unwrap_or(false);
        let recycle = node.bool_prop("recycle").unwrap_or(true);
        let stop_thread = node.bool_prop("stopThread").unwrap_or(false);
        let share_mode = node.prop("shareMode").unwrap_or("shareMode.all").trim();
        let delimiter = node
            .prop("delimiter")
            .map(str::trim)
            .filter(|d| !d.is_empty() && *d != ",")
            .and_then(|d| d.chars().next());

        let has_header = if variable_names.is_empty() {
            // JMeter reads column names from the first line — same as loadr.
            true
        } else if ignore_first_line {
            self.warn(
                &label,
                "CSV declares variable names AND skips the first line; loadr reads \
                 column names from the header row — ensure the header matches the \
                 JMeter variable names",
            );
            true
        } else {
            self.warn(
                &label,
                format!(
                    "CSV declares variable names ({}) so the file is assumed to have \
                     no header row (`has_header: false`); `${{name}}` references are \
                     rewritten to `${{data.{source}.<name>}}` — verify column mapping",
                    variable_names.join(", ")
                ),
            );
            false
        };

        let mode = if share_mode.ends_with("all") {
            DataMode::Shared
        } else {
            DataMode::PerVu
        };
        let on_eof = if !recycle && stop_thread {
            OnEof::Stop
        } else {
            OnEof::Recycle
        };

        self.plan.data.insert(
            source.clone(),
            DataSource::Csv {
                path: filename.into(),
                mode,
                on_eof,
                delimiter,
                has_header,
            },
        );
        for name in variable_names {
            self.rewrites
                .push((format!("${{{name}}}"), format!("${{data.{source}.{name}}}")));
        }
    }

    /// Apply `${name}` rewrites and warn about JMeter `${__function}` calls.
    fn rewrite(&mut self, s: &str) -> String {
        let mut out = s.to_string();
        for (from, to) in &self.rewrites {
            out = out.replace(from, to);
        }
        for cap in regex_jmeter_function().captures_iter(&out) {
            if let Some(name) = cap.get(1) {
                let name = name.as_str().to_string();
                if self.warned_functions.insert(name.clone()) {
                    self.warnings.push(ConversionWarning {
                        element: format!("${{__{name}}}"),
                        message: "JMeter function has no loadr equivalent; \
                                  replace it with a variable, data source or JS step"
                            .into(),
                    });
                }
            }
        }
        out
    }

    // -- thread groups --------------------------------------------------------

    fn convert_thread_group(&mut self, tg: &Node, ht: Option<&Node>) {
        let label = tg.label();
        let name = unique_name(&mut self.scenario_names, slugify(tg.testname(), "scenario"));

        let threads = self
            .parse_u64(&label, "num_threads", tg.prop("ThreadGroup.num_threads"))
            .unwrap_or(1)
            .max(1);
        let ramp = self
            .parse_u64(&label, "ramp_time", tg.prop("ThreadGroup.ramp_time"))
            .unwrap_or(0);
        let scheduler = tg.bool_prop("ThreadGroup.scheduler").unwrap_or(false);
        let sched_duration = self
            .parse_u64(&label, "duration", tg.prop("ThreadGroup.duration"))
            .unwrap_or(0);

        let (loops, forever) = match tg.element_prop("ThreadGroup.main_controller") {
            Some(lc) => {
                let cont = lc
                    .bool_prop("LoopController.continue_forever")
                    .unwrap_or(false);
                let raw = lc.prop("LoopController.loops").unwrap_or("1").trim();
                let n: i64 = raw.parse().unwrap_or(1);
                (n.max(1) as u64, cont || n == -1)
            }
            None => (1, false),
        };

        let mut scenario = Scenario::default();

        if forever || (scheduler && sched_duration > 0) {
            let duration = if scheduler && sched_duration > 0 {
                sched_duration
            } else {
                self.warn(
                    &label,
                    "thread group loops forever without a scheduler duration; \
                     defaulting the scenario duration to 60s",
                );
                60
            };
            if ramp == 0 {
                scenario.executor = ExecutorKind::ConstantVus;
                scenario.vus = Some(threads);
                scenario.duration = Some(Dur::from_secs(duration));
            } else {
                scenario.executor = ExecutorKind::RampingVus;
                scenario.stages.push(Stage {
                    duration: Dur::from_secs(ramp),
                    target: threads as f64,
                });
                let hold = duration.saturating_sub(ramp);
                if hold > 0 {
                    scenario.stages.push(Stage {
                        duration: Dur::from_secs(hold),
                        target: threads as f64,
                    });
                }
            }
        } else {
            scenario.executor = ExecutorKind::PerVuIterations;
            scenario.vus = Some(threads);
            scenario.iterations = Some(loops);
            if ramp > 0 {
                self.warn(
                    &label,
                    "ramp-up time is ignored for iteration-based thread groups",
                );
            }
        }

        let mut steps = Vec::new();
        if let Some(ht) = ht {
            self.convert_controller_children(&ht.children, &mut steps, &mut scenario);
        }
        if steps.is_empty() {
            self.warn(
                &label,
                "thread group has no samplers; a placeholder request was NOT added \
                 and the scenario was skipped",
            );
            self.scenario_names.remove(&name);
            return;
        }
        scenario.flow = steps;
        self.plan.scenarios.insert(name, scenario);
    }

    /// Convert the children of a thread group or controller into flow steps.
    fn convert_controller_children(
        &mut self,
        nodes: &[Node],
        steps: &mut Vec<Step>,
        scenario: &mut Scenario,
    ) {
        for (node, ht) in pairs(nodes) {
            if !node.enabled() {
                self.warn(node.label(), "element is disabled; skipped");
                continue;
            }
            match node.tag.as_str() {
                "HTTPSamplerProxy" => {
                    let new_steps = self.convert_sampler(node, ht);
                    steps.extend(new_steps);
                }
                "TransactionController" => {
                    let mut inner = Vec::new();
                    if let Some(ht) = ht {
                        self.convert_controller_children(&ht.children, &mut inner, scenario);
                    }
                    steps.push(Step::Group(GroupStep {
                        name: non_empty(node.testname())
                            .unwrap_or("transaction")
                            .to_string(),
                        steps: inner,
                    }));
                }
                "LoopController" => {
                    let mut inner = Vec::new();
                    if let Some(ht) = ht {
                        self.convert_controller_children(&ht.children, &mut inner, scenario);
                    }
                    let raw = node.prop("LoopController.loops").unwrap_or("1").trim();
                    let n: i64 = raw.parse().unwrap_or(1);
                    let times = if n < 0 {
                        self.warn(
                            node.label(),
                            "loop controller set to loop forever; children included once",
                        );
                        1
                    } else if n > 10 {
                        self.warn(
                            node.label(),
                            format!(
                                "loop count {n} exceeds 10; children included once \
                                     — wrap them in a JS loop if exact repetition matters"
                            ),
                        );
                        1
                    } else {
                        n.max(1) as usize
                    };
                    let mut replicated = Vec::with_capacity(inner.len() * times);
                    for _ in 0..times {
                        replicated.extend(inner.iter().cloned());
                    }
                    steps.push(Step::Group(GroupStep {
                        name: non_empty(node.testname()).unwrap_or("loop").to_string(),
                        steps: replicated,
                    }));
                }
                "HeaderManager" => {
                    for (k, v) in parse_header_manager(node) {
                        let v = self.rewrite(&v);
                        self.plan.defaults.http.headers.insert(k, v);
                    }
                }
                "CookieManager" => self.convert_cookie_manager(node),
                "CSVDataSet" | "Arguments" => {} // handled in prescan
                "ConstantTimer" => {
                    if let Some(tt) = self.parse_constant_timer(node) {
                        scenario.think_time = Some(tt);
                    }
                }
                "UniformRandomTimer" => {
                    if let Some(tt) = self.parse_uniform_timer(node) {
                        scenario.think_time = Some(tt);
                    }
                }
                "GaussianRandomTimer" => {
                    if let Some(tt) = self.parse_gaussian_timer(node) {
                        scenario.think_time = Some(tt);
                    }
                }
                "ConstantThroughputTimer" => {
                    let per_minute = node
                        .prop("throughput")
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .unwrap_or(0.0);
                    if per_minute > 0.0 {
                        scenario.pacing = Some(PacingSpec {
                            iterations_per_second: per_minute / 60.0,
                        });
                    } else {
                        self.warn(
                            node.label(),
                            "constant throughput timer has no rate; skipped",
                        );
                    }
                }
                other => {
                    self.warn(
                        node.label(),
                        format!(
                            "unsupported element <{other}>; its children (if any) \
                             were converted inline"
                        ),
                    );
                    if let Some(ht) = ht {
                        self.convert_controller_children(&ht.children, steps, scenario);
                    }
                }
            }
        }
    }

    // -- samplers -------------------------------------------------------------

    /// Convert one HTTP sampler (plus its scoped children) into flow steps:
    /// any sampler-scoped timers (as think time before the request) + the request.
    fn convert_sampler(&mut self, node: &Node, ht: Option<&Node>) -> Vec<Step> {
        let label = node.label();
        let method = node
            .prop("HTTPSampler.method")
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let protocol = node
            .prop("HTTPSampler.protocol")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .unwrap_or("http")
            .to_ascii_lowercase();
        let domain = node
            .prop("HTTPSampler.domain")
            .unwrap_or("")
            .trim()
            .to_string();
        let port = node
            .prop("HTTPSampler.port")
            .unwrap_or("")
            .trim()
            .to_string();
        let path_raw = node
            .prop("HTTPSampler.path")
            .unwrap_or("")
            .trim()
            .to_string();
        let content_encoding = node
            .prop("HTTPSampler.contentEncoding")
            .unwrap_or("")
            .trim()
            .to_string();
        let post_body_raw = node.bool_prop("HTTPSampler.postBodyRaw").unwrap_or(false);
        let follow_redirects = node
            .bool_prop("HTTPSampler.follow_redirects")
            .or_else(|| node.bool_prop("HTTPSampler.auto_redirects"))
            .unwrap_or(true);
        if node.bool_prop("HTTPSampler.image_parser").unwrap_or(false) {
            self.warn(
                &label,
                "`download embedded resources` is not supported; only the main \
                 request is converted",
            );
        }
        if !content_encoding.is_empty() && !content_encoding.eq_ignore_ascii_case("utf-8") {
            self.warn(
                &label,
                format!("content encoding `{content_encoding}` is ignored (loadr sends UTF-8)"),
            );
        }

        let mut req = RequestStep {
            name: non_empty(node.testname()).map(str::to_string),
            method: Some(method.clone()),
            ..Default::default()
        };
        if !follow_redirects {
            req.follow_redirects = Some(false);
        }

        // URL: absolute when a domain is set, otherwise relative.
        let path = if path_raw.is_empty() {
            "/".to_string()
        } else if path_raw.starts_with('/') || path_raw.starts_with("${") {
            path_raw
        } else {
            format!("/{path_raw}")
        };
        if domain.is_empty() {
            req.url = self.rewrite(&path);
        } else {
            let default_port = matches!(
                (protocol.as_str(), port.as_str()),
                (_, "") | ("http", "80") | ("https", "443")
            );
            let base = if default_port {
                format!("{protocol}://{domain}")
            } else {
                format!("{protocol}://{domain}:{port}")
            };
            req.url = self.rewrite(&format!("{base}{path}"));
            self.bases.insert(base);
        }

        // Arguments: query params for body-less methods, form/raw body otherwise.
        let args = node
            .element_prop("HTTPsampler.Arguments")
            .or_else(|| node.element_prop("HTTPSampler.Arguments"))
            .and_then(|a| a.collection_prop("Arguments.arguments"));
        if let Some(coll) = args {
            if post_body_raw {
                let raw: String = coll
                    .children
                    .iter()
                    .filter(|c| c.tag == "elementProp")
                    .filter_map(|c| c.prop("Argument.value"))
                    .collect::<Vec<_>>()
                    .join("");
                if !raw.is_empty() {
                    req.body = Some(Body::Text(self.rewrite(&raw)));
                }
            } else {
                let mut kv: IndexMap<String, String> = IndexMap::new();
                for arg in coll.children.iter().filter(|c| c.tag == "elementProp") {
                    let name = arg
                        .prop("Argument.name")
                        .or_else(|| arg.attr("name"))
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let value = arg.prop("Argument.value").unwrap_or("").to_string();
                    let value = self.rewrite(&value);
                    kv.insert(name, value);
                }
                if !kv.is_empty() {
                    if matches!(method.as_str(), "POST" | "PUT" | "PATCH") {
                        req.body = Some(Body::Spec(BodySpec {
                            form: Some(kv),
                            ..Default::default()
                        }));
                    } else {
                        req.params = kv;
                    }
                }
            }
        }

        // Sampler-scoped children: headers, assertions, extractors, timers.
        let mut pre_steps: Vec<Step> = Vec::new();
        if let Some(ht) = ht {
            for (child, _) in pairs(&ht.children) {
                if !child.enabled() {
                    self.warn(child.label(), "element is disabled; skipped");
                    continue;
                }
                match child.tag.as_str() {
                    "HeaderManager" => {
                        for (k, v) in parse_header_manager(child) {
                            let v = self.rewrite(&v);
                            req.headers.insert(k, v);
                        }
                    }
                    "ResponseAssertion" => self.convert_response_assertion(child, &mut req),
                    "DurationAssertion" => {
                        let ms = child
                            .prop("DurationAssertion.duration")
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .unwrap_or(0);
                        if ms > 0 {
                            req.assert.push(Condition::Duration {
                                name: non_empty(child.testname()).map(str::to_string),
                                max: Dur::from_millis(ms),
                                on_failure: None,
                            });
                        }
                    }
                    "SizeAssertion" => self.convert_size_assertion(child, &mut req),
                    "JSONPathAssertion" => self.convert_jsonpath_assertion(child, &mut req),
                    "XPathAssertion" => {
                        let expr = child.prop("XPath.xpath").unwrap_or("").trim().to_string();
                        if expr.is_empty() {
                            self.warn(child.label(), "XPath assertion has no expression; skipped");
                        } else {
                            let negate = child.bool_prop("XPath.negate").unwrap_or(false);
                            req.assert.push(Condition::Xpath {
                                name: non_empty(child.testname()).map(str::to_string),
                                expression: expr,
                                equals: None,
                                exists: Some(!negate),
                                on_failure: None,
                            });
                        }
                    }
                    "RegexExtractor" => self.convert_regex_extractor(child, &mut req),
                    "JSONPostProcessor" => self.convert_json_extractor(child, &mut req),
                    "XPathExtractor" => {
                        let name = child.prop("XPathExtractor.refname").unwrap_or("").trim();
                        let expr = child.prop("XPathExtractor.xpathQuery").unwrap_or("").trim();
                        if name.is_empty() || expr.is_empty() {
                            self.warn(child.label(), "XPath extractor incomplete; skipped");
                        } else {
                            req.extract.push(Extractor::Xpath {
                                name: name.to_string(),
                                expression: expr.to_string(),
                                default: non_empty(
                                    child.prop("XPathExtractor.default").unwrap_or(""),
                                )
                                .map(str::to_string),
                            });
                        }
                    }
                    "BoundaryExtractor" => self.convert_boundary_extractor(child, &mut req),
                    "ConstantTimer" => {
                        if let Some(tt) = self.parse_constant_timer(child) {
                            pre_steps.push(Step::ThinkTime(tt));
                        }
                    }
                    "UniformRandomTimer" => {
                        if let Some(tt) = self.parse_uniform_timer(child) {
                            pre_steps.push(Step::ThinkTime(tt));
                        }
                    }
                    "GaussianRandomTimer" => {
                        if let Some(tt) = self.parse_gaussian_timer(child) {
                            pre_steps.push(Step::ThinkTime(tt));
                        }
                    }
                    other => self.warn(
                        child.label(),
                        format!("unsupported element <{other}> under sampler; skipped"),
                    ),
                }
            }
        }

        pre_steps.push(Step::Request(Box::new(req)));
        pre_steps
    }

    // -- assertions -------------------------------------------------------------

    fn convert_response_assertion(&mut self, node: &Node, req: &mut RequestStep) {
        let label = node.label();
        let name = non_empty(node.testname()).map(str::to_string);
        let field = node
            .prop("Assertion.test_field")
            .unwrap_or("")
            .trim()
            .to_string();
        let test_type = node.int_prop("Assertion.test_type").unwrap_or(16);
        let negate = test_type & 4 != 0;
        let kind = test_type & !(4 | 32); // strip NOT and OR flags
        let values: Vec<String> = node
            .collection_prop("Asserion.test_strings") // sic: JMeter's historic typo
            .or_else(|| node.collection_prop("Assertion.test_strings"))
            .map(|coll| {
                coll.children
                    .iter()
                    .filter(|c| c.tag == "stringProp")
                    .map(|c| c.text.clone())
                    .collect()
            })
            .unwrap_or_default();
        if values.is_empty() {
            self.warn(label, "response assertion has no patterns to test; skipped");
            return;
        }

        match field.as_str() {
            "Assertion.response_code" => {
                for v in &values {
                    let trimmed = v.trim();
                    if negate {
                        self.warn(
                            &label,
                            "NOT on a response-code assertion is not supported; \
                             the condition was added without negation",
                        );
                    }
                    let cond = if kind == 8 {
                        match trimmed.parse::<i64>() {
                            Ok(code) => Condition::Status {
                                name: name.clone(),
                                equals: Some(code),
                                one_of: None,
                                matches: None,
                                on_failure: None,
                            },
                            Err(_) => Condition::Status {
                                name: name.clone(),
                                equals: None,
                                one_of: None,
                                matches: Some(regex::escape(trimmed)),
                                on_failure: None,
                            },
                        }
                    } else {
                        let pattern = if kind == 16 {
                            regex::escape(trimmed)
                        } else {
                            trimmed.to_string()
                        };
                        Condition::Status {
                            name: name.clone(),
                            equals: None,
                            one_of: None,
                            matches: Some(pattern),
                            on_failure: None,
                        }
                    };
                    req.assert.push(cond);
                }
            }
            "Assertion.response_data" | "Assertion.response_data_as_document" => {
                for v in &values {
                    let cond = match kind {
                        16 => Condition::BodyContains {
                            name: name.clone(),
                            value: v.clone(),
                            negate,
                            on_failure: None,
                        },
                        8 => Condition::BodyMatches {
                            name: name.clone(),
                            pattern: format!("(?s)^{}$", regex::escape(v)),
                            negate,
                            on_failure: None,
                        },
                        // 1 = matches (regex, whole), 2 = contains (regex, anywhere)
                        _ => Condition::BodyMatches {
                            name: name.clone(),
                            pattern: v.clone(),
                            negate,
                            on_failure: None,
                        },
                    };
                    req.assert.push(cond);
                }
            }
            other => {
                self.warn(
                    label,
                    format!(
                        "response assertion on field `{other}` is not supported \
                         (only response code and response body); skipped"
                    ),
                );
            }
        }
    }

    fn convert_size_assertion(&mut self, node: &Node, req: &mut RequestStep) {
        let name = non_empty(node.testname()).map(str::to_string);
        let size = node
            .prop("SizeAssertion.size")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let op = node.int_prop("SizeAssertion.operator").unwrap_or(1);
        let (min, max, equals) = match op {
            1 => (None, None, Some(size)),
            3 => (Some(size.saturating_add(1)), None, None),
            5 => (Some(size), None, None),
            4 => (None, Some(size.saturating_sub(1)), None),
            6 => (None, Some(size), None),
            other => {
                self.warn(
                    node.label(),
                    format!(
                        "size assertion operator {other} (e.g. `!=`) is not supported; skipped"
                    ),
                );
                return;
            }
        };
        req.assert.push(Condition::Size {
            name,
            min,
            max,
            equals,
            on_failure: None,
        });
    }

    fn convert_jsonpath_assertion(&mut self, node: &Node, req: &mut RequestStep) {
        let label = node.label();
        let expr = node.prop("JSON_PATH").unwrap_or("").trim().to_string();
        if expr.is_empty() {
            self.warn(label, "JSONPath assertion has no expression; skipped");
            return;
        }
        let validate_value = node.bool_prop("JSONVALIDATION").unwrap_or(false);
        let invert = node.bool_prop("INVERT").unwrap_or(false);
        let is_regex = node.bool_prop("ISREGEX").unwrap_or(false);
        let expected = node.prop("EXPECTED_VALUE").unwrap_or("").to_string();

        let equals = if validate_value && !is_regex {
            Some(
                serde_json::from_str::<serde_json::Value>(expected.trim())
                    .unwrap_or(serde_json::Value::String(expected.clone())),
            )
        } else {
            if validate_value && is_regex {
                self.warn(
                    &label,
                    "regex value matching in a JSONPath assertion is not supported; \
                     converted to an existence check",
                );
            }
            None
        };
        if invert && equals.is_some() {
            self.warn(
                &label,
                "inverted JSONPath value assertion is not supported; negation dropped",
            );
        }
        let exists = if equals.is_some() {
            None
        } else {
            Some(!invert)
        };
        req.assert.push(Condition::Jsonpath {
            name: non_empty(node.testname()).map(str::to_string),
            expression: expr,
            equals,
            exists,
            on_failure: None,
        });
    }

    // -- extractors ---------------------------------------------------------------

    fn convert_regex_extractor(&mut self, node: &Node, req: &mut RequestStep) {
        let label = node.label();
        let name = node
            .prop("RegexExtractor.refname")
            .unwrap_or("")
            .trim()
            .to_string();
        let expr = node.prop("RegexExtractor.regex").unwrap_or("").to_string();
        if name.is_empty() || expr.is_empty() {
            self.warn(
                label,
                "regex extractor is missing a name or expression; skipped",
            );
            return;
        }
        let template = node.prop("RegexExtractor.template").unwrap_or("$1$").trim();
        let group = regex_template_group()
            .captures(template)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<usize>().ok());
        if group.is_none() && !template.is_empty() {
            self.warn(
                &label,
                format!("regex template `{template}` is not a simple `$N$`; using group 1"),
            );
        }
        let index = self.match_number_index(&label, node.prop("RegexExtractor.match_number"));
        req.extract.push(Extractor::Regex {
            name,
            expression: expr,
            group,
            default: non_empty(node.prop("RegexExtractor.default").unwrap_or(""))
                .map(str::to_string),
            index,
        });
    }

    fn convert_json_extractor(&mut self, node: &Node, req: &mut RequestStep) {
        let label = node.label();
        let names: Vec<&str> = node
            .prop("JSONPostProcessor.referenceNames")
            .unwrap_or("")
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let exprs: Vec<&str> = node
            .prop("JSONPostProcessor.jsonPathExprs")
            .unwrap_or("")
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let defaults: Vec<&str> = node
            .prop("JSONPostProcessor.defaultValues")
            .unwrap_or("")
            .split(';')
            .map(str::trim)
            .collect();
        if names.is_empty() || names.len() != exprs.len() {
            self.warn(
                label,
                "JSON extractor has mismatched reference names / expressions; skipped",
            );
            return;
        }
        let index = self.match_number_index(&label, node.prop("JSONPostProcessor.match_numbers"));
        for (i, (name, expr)) in names.iter().zip(exprs.iter()).enumerate() {
            req.extract.push(Extractor::Jsonpath {
                name: (*name).to_string(),
                expression: (*expr).to_string(),
                default: defaults
                    .get(i)
                    .copied()
                    .filter(|d| !d.is_empty())
                    .map(str::to_string),
                index,
            });
        }
    }

    fn convert_boundary_extractor(&mut self, node: &Node, req: &mut RequestStep) {
        let label = node.label();
        let name = node
            .prop("BoundaryExtractor.refname")
            .unwrap_or("")
            .trim()
            .to_string();
        let left = node
            .prop("BoundaryExtractor.lboundary")
            .unwrap_or("")
            .to_string();
        let right = node
            .prop("BoundaryExtractor.rboundary")
            .unwrap_or("")
            .to_string();
        if name.is_empty() || (left.is_empty() && right.is_empty()) {
            self.warn(label, "boundary extractor incomplete; skipped");
            return;
        }
        let index = self.match_number_index(&label, node.prop("BoundaryExtractor.match_number"));
        req.extract.push(Extractor::Boundary {
            name,
            left,
            right,
            default: non_empty(node.prop("BoundaryExtractor.default").unwrap_or(""))
                .map(str::to_string),
            index,
        });
    }

    /// JMeter match numbers: 0 = random, 1 = first, -1 = all, N>1 = unsupported.
    fn match_number_index(&mut self, label: &str, raw: Option<&str>) -> Option<MatchIndex> {
        match raw.map(str::trim) {
            None | Some("") | Some("1") => None,
            Some("0") => Some(MatchIndex::Random),
            Some("-1") => Some(MatchIndex::All),
            Some(other) => {
                self.warn(
                    label,
                    format!("match number `{other}` is not supported; using the first match"),
                );
                None
            }
        }
    }

    // -- timers, cookies, misc -----------------------------------------------------

    fn parse_constant_timer(&mut self, node: &Node) -> Option<ThinkTimeSpec> {
        let ms = node
            .prop("ConstantTimer.delay")
            .and_then(|v| v.trim().parse::<u64>().ok());
        match ms {
            Some(ms) => Some(ThinkTimeSpec::Constant {
                duration: Dur::from_millis(ms),
            }),
            None => {
                self.warn(
                    node.label(),
                    "constant timer delay is not a number; skipped",
                );
                None
            }
        }
    }

    fn parse_uniform_timer(&mut self, node: &Node) -> Option<ThinkTimeSpec> {
        let offset = node
            .prop("ConstantTimer.delay")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let range = node
            .prop("RandomTimer.range")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            .max(0.0) as u64;
        Some(ThinkTimeSpec::Uniform {
            min: Dur::from_millis(offset),
            max: Dur::from_millis(offset + range),
        })
    }

    fn parse_gaussian_timer(&mut self, node: &Node) -> Option<ThinkTimeSpec> {
        let offset = node
            .prop("ConstantTimer.delay")
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(0);
        let deviation = node
            .prop("RandomTimer.range")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(100.0)
            .max(0.0) as u64;
        Some(ThinkTimeSpec::Gaussian {
            mean: Dur::from_millis(offset),
            std_dev: Dur::from_millis(deviation),
        })
    }

    fn convert_cookie_manager(&mut self, node: &Node) {
        // Automatic per-VU cookie handling is loadr's default already.
        self.plan.defaults.http.cookies = true;
        if node
            .bool_prop("CookieManager.clearEachIteration")
            .unwrap_or(false)
        {
            self.warn(
                node.label(),
                "`clear cookies each iteration` has no loadr equivalent; \
                 the cookie jar persists for the lifetime of each VU",
            );
        }
    }

    fn parse_u64(&mut self, label: &str, field: &str, raw: Option<&str>) -> Option<u64> {
        let raw = raw.map(str::trim).filter(|s| !s.is_empty())?;
        match raw.parse::<u64>() {
            Ok(v) => Some(v),
            Err(_) => {
                self.warn(
                    label,
                    format!("`{field}` value `{raw}` is not a plain number; using a default"),
                );
                None
            }
        }
    }

    // -- base URL hoisting -----------------------------------------------------------

    fn hoist_base_url(&mut self) {
        if self.bases.len() != 1 {
            return;
        }
        let Some(base) = self.bases.iter().next().cloned() else {
            return;
        };
        for scenario in self.plan.scenarios.values_mut() {
            relativize_steps(&mut scenario.flow, &base);
        }
        self.plan.defaults.http.base_url = Some(base);
    }
}

fn relativize_steps(steps: &mut [Step], base: &str) {
    for step in steps {
        match step {
            Step::Request(req) => {
                if let Some(rest) = req.url.strip_prefix(base) {
                    req.url = if rest.is_empty() {
                        "/".to_string()
                    } else {
                        rest.to_string()
                    };
                }
            }
            Step::Group(g) => relativize_steps(&mut g.steps, base),
            _ => {}
        }
    }
}

fn parse_header_manager(node: &Node) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(coll) = node.collection_prop("HeaderManager.headers") {
        for h in coll.children.iter().filter(|c| c.tag == "elementProp") {
            let name = h.prop("Header.name").unwrap_or("").trim().to_string();
            if name.is_empty() {
                continue;
            }
            let value = h.prop("Header.value").unwrap_or("").trim().to_string();
            out.push((name, value));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

fn slugify(s: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed
    }
}

fn unique_name(taken: &mut BTreeSet<String>, base: String) -> String {
    if taken.insert(base.clone()) {
        return base;
    }
    let mut i = 2;
    loop {
        let cand = format!("{base}-{i}");
        if taken.insert(cand.clone()) {
            return cand;
        }
        i += 1;
    }
}

fn regex_jmeter_function() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"\$\{__(\w+)").unwrap_or_else(|_| unreachable!("static regex"))
    })
}

fn regex_template_group() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"^\$(\d+)\$$").unwrap_or_else(|_| unreachable!("static regex"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_normalizes_names() {
        assert_eq!(slugify("Browse Shop", "x"), "browse-shop");
        assert_eq!(slugify("  API / v2 (load) ", "x"), "api-v2-load");
        assert_eq!(slugify("***", "fallback"), "fallback");
    }

    #[test]
    fn unique_name_dedupes() {
        let mut taken = BTreeSet::new();
        assert_eq!(unique_name(&mut taken, "a".into()), "a");
        assert_eq!(unique_name(&mut taken, "a".into()), "a-2");
        assert_eq!(unique_name(&mut taken, "a".into()), "a-3");
    }

    #[test]
    fn nested_double_prop_is_read() {
        let xml = r#"<?xml version="1.0"?>
<jmeterTestPlan><hashTree>
  <TestPlan testname="t"/>
  <hashTree>
    <ThreadGroup testname="g">
      <elementProp name="ThreadGroup.main_controller" elementType="LoopController">
        <stringProp name="LoopController.loops">-1</stringProp>
      </elementProp>
      <stringProp name="ThreadGroup.num_threads">2</stringProp>
      <stringProp name="ThreadGroup.ramp_time">0</stringProp>
      <boolProp name="ThreadGroup.scheduler">true</boolProp>
      <stringProp name="ThreadGroup.duration">60</stringProp>
    </ThreadGroup>
    <hashTree>
      <ConstantThroughputTimer testname="pace">
        <doubleProp>
          <name>throughput</name>
          <value>30.0</value>
        </doubleProp>
      </ConstantThroughputTimer>
      <hashTree/>
      <HTTPSamplerProxy testname="ping">
        <stringProp name="HTTPSampler.domain">example.com</stringProp>
        <stringProp name="HTTPSampler.protocol">https</stringProp>
        <stringProp name="HTTPSampler.path">/ping</stringProp>
        <stringProp name="HTTPSampler.method">GET</stringProp>
      </HTTPSamplerProxy>
      <hashTree/>
    </hashTree>
  </hashTree>
</hashTree></jmeterTestPlan>"#;
        let conv = convert_jmx(xml).expect("convert");
        let scenario = conv.plan.scenarios.get("g").expect("scenario");
        let pacing = scenario.pacing.expect("pacing from doubleProp");
        assert!((pacing.iterations_per_second - 0.5).abs() < 1e-9);
    }

    #[test]
    fn xml_entities_are_resolved() {
        let xml = r#"<?xml version="1.0"?>
<jmeterTestPlan><hashTree>
  <TestPlan testname="A &amp; B"/>
  <hashTree>
    <ThreadGroup testname="g">
      <stringProp name="ThreadGroup.num_threads">1</stringProp>
      <stringProp name="ThreadGroup.ramp_time">0</stringProp>
    </ThreadGroup>
    <hashTree>
      <HTTPSamplerProxy testname="q">
        <stringProp name="HTTPSampler.domain">example.com</stringProp>
        <stringProp name="HTTPSampler.protocol">https</stringProp>
        <stringProp name="HTTPSampler.path">/search?q=a&amp;b</stringProp>
      </HTTPSamplerProxy>
      <hashTree/>
    </hashTree>
  </hashTree>
</hashTree></jmeterTestPlan>"#;
        let conv = convert_jmx(xml).expect("convert");
        assert_eq!(conv.plan.name.as_deref(), Some("A & B"));
        let scenario = conv.plan.scenarios.get("g").expect("scenario");
        match &scenario.flow[0] {
            Step::Request(r) => assert_eq!(r.url, "/search?q=a&b"),
            other => panic!("expected request, got {other:?}"),
        }
    }
}
