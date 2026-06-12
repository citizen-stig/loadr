//! k6 JavaScript → loadr [`TestPlan`] conversion.
//!
//! No JS engine is used: the script is analyzed with comment-stripping, brace
//! matching and regular expressions over the source. Statements that cannot be
//! recognized are preserved by copying the original script into `js.script`
//! (with k6 imports stripped) and reported as warnings.

use std::collections::BTreeMap;

use indexmap::IndexMap;
use loadr_config::{
    Body, BodySpec, Condition, CustomMetric, Dur, ExecutorKind, GroupStep, JsConfig,
    MetricKindSpec, RequestStep, Scenario, Stage, Step, TestPlan, ThinkTimeSpec, ThresholdEntry,
    ThresholdList,
};

use crate::{Conversion, ConversionWarning, ConvertError};

/// Convert a k6 JavaScript script into a loadr test plan.
pub fn convert_k6(js: &str) -> Result<Conversion, ConvertError> {
    let mut cv = K6Converter {
        original: js.to_string(),
        stripped: strip_comments(js),
        ..Default::default()
    };
    cv.scan_imports();
    cv.scan_metrics();
    cv.apply_options()?;
    cv.convert_functions()?;
    cv.finish();
    Ok(Conversion {
        plan: cv.plan,
        warnings: cv.warnings,
    })
}

#[derive(Default)]
struct K6Converter {
    original: String,
    /// Comment-free copy used for all parsing.
    stripped: String,
    plan: TestPlan,
    warnings: Vec<ConversionWarning>,
    /// Lines we could not convert into YAML steps.
    unconverted: Vec<String>,
    /// setup()/teardown() present.
    has_lifecycle: bool,
    /// exec function names that could not be found/converted.
    needs_js_module: bool,
}

impl K6Converter {
    fn warn(&mut self, element: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(ConversionWarning {
            element: element.into(),
            message: message.into(),
        });
    }

    // -- imports & metrics ---------------------------------------------------

    fn scan_imports(&mut self) {
        let re = static_regex(r#"(?m)^\s*import\s+[^;'"]*['"]([^'"]+)['"]"#);
        let mut unknown = Vec::new();
        for cap in re.captures_iter(&self.stripped) {
            let module = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            match module {
                "k6" | "k6/http" | "k6/metrics" => {}
                other => unknown.push(other.to_string()),
            }
        }
        for module in unknown {
            self.warn(
                format!("import '{module}'"),
                "unrecognized import; its functionality is not converted — \
                 review the generated plan",
            );
        }
    }

    fn scan_metrics(&mut self) {
        let re = static_regex(r#"new\s+(Trend|Counter|Rate|Gauge)\s*\(\s*['"]([^'"]+)['"]"#);
        let stripped = self.stripped.clone();
        for cap in re.captures_iter(&stripped) {
            let (Some(kind), Some(name)) = (cap.get(1), cap.get(2)) else {
                continue;
            };
            let kind = match kind.as_str() {
                "Trend" => MetricKindSpec::Trend,
                "Counter" => MetricKindSpec::Counter,
                "Rate" => MetricKindSpec::Rate,
                _ => MetricKindSpec::Gauge,
            };
            self.plan.metrics.insert(
                name.as_str().to_string(),
                CustomMetric {
                    kind,
                    description: None,
                    time: false,
                },
            );
        }
    }

    // -- options -------------------------------------------------------------

    fn apply_options(&mut self) -> Result<(), ConvertError> {
        let re = static_regex(r"export\s+(?:const|let|var)\s+options\s*=\s*");
        let Some(m) = re.find(&self.stripped) else {
            // No options: k6 default is 1 VU x 1 iteration.
            self.plan.scenarios.insert(
                "default".into(),
                Scenario {
                    executor: ExecutorKind::PerVuIterations,
                    vus: Some(1),
                    iterations: Some(1),
                    ..Default::default()
                },
            );
            self.warn(
                "options",
                "no `export const options` found; defaulting to 1 VU, 1 iteration",
            );
            return Ok(());
        };
        let rest = &self.stripped[m.end()..];
        let open_rel = rest
            .find('{')
            .ok_or_else(|| ConvertError::Options("`options` is not an object literal".into()))?;
        let open = m.end() + open_rel;
        let close = match_delim(&self.stripped, open)
            .ok_or_else(|| ConvertError::Options("unbalanced braces in `options` object".into()))?;
        let literal = self.stripped[open..=close].to_string();
        let value = js_object_to_json(&literal)
            .map_err(|e| ConvertError::Options(format!("cannot interpret options: {e}")))?;
        let obj = value
            .as_object()
            .cloned()
            .ok_or_else(|| ConvertError::Options("options is not an object".into()))?;

        // Thresholds first (independent of executors).
        if let Some(thresholds) = obj.get("thresholds").and_then(|v| v.as_object()) {
            for (metric, spec) in thresholds {
                if let Some(list) = self.threshold_list(metric, spec) {
                    self.plan.thresholds.insert(metric.clone(), list);
                }
            }
        }

        if let Some(scenarios) = obj.get("scenarios").and_then(|v| v.as_object()) {
            for (name, sc) in scenarios {
                match sc.as_object() {
                    Some(map) => {
                        let scenario = self.scenario_from_k6(name, map);
                        self.plan.scenarios.insert(name.clone(), scenario);
                    }
                    None => self.warn(
                        format!("scenarios.{name}"),
                        "scenario is not an object; skipped",
                    ),
                }
            }
        } else {
            // Shorthand executors.
            let mut scenario = Scenario::default();
            let vus = obj.get("vus").and_then(|v| v.as_u64());
            if let Some(stages) = obj.get("stages").and_then(|v| v.as_array()) {
                scenario.executor = ExecutorKind::RampingVus;
                scenario.start_vus = vus;
                scenario.stages = self.stages_from_json("options.stages", stages);
            } else if let Some(iterations) = obj.get("iterations").and_then(|v| v.as_u64()) {
                scenario.executor = ExecutorKind::SharedIterations;
                scenario.vus = Some(vus.unwrap_or(1));
                scenario.iterations = Some(iterations);
            } else if let Some(duration) = obj.get("duration") {
                scenario.executor = ExecutorKind::ConstantVus;
                scenario.vus = Some(vus.unwrap_or(1));
                scenario.duration = self.dur_field("options.duration", duration);
            } else {
                scenario.executor = ExecutorKind::PerVuIterations;
                scenario.vus = Some(vus.unwrap_or(1));
                scenario.iterations = Some(1);
                if vus.is_some() {
                    self.warn(
                        "options",
                        "`vus` without `duration`/`iterations`/`stages`; \
                         defaulting to 1 iteration per VU",
                    );
                }
            }
            self.plan.scenarios.insert("default".into(), scenario);
        }

        for key in obj.keys() {
            match key.as_str() {
                "vus" | "duration" | "iterations" | "stages" | "scenarios" | "thresholds" => {}
                other => self.warn(
                    format!("options.{other}"),
                    "option is not supported by the converter; review manually",
                ),
            }
        }
        Ok(())
    }

    fn threshold_list(&mut self, metric: &str, spec: &serde_json::Value) -> Option<ThresholdList> {
        match spec {
            serde_json::Value::String(s) => Some(ThresholdList::Single(s.clone())),
            serde_json::Value::Array(items) => {
                let mut entries = Vec::new();
                for item in items {
                    match item {
                        serde_json::Value::String(s) => {
                            entries.push(ThresholdEntry::Expr(s.clone()))
                        }
                        serde_json::Value::Object(o) => {
                            let Some(threshold) = o.get("threshold").and_then(|v| v.as_str())
                            else {
                                self.warn(
                                    format!("thresholds.{metric}"),
                                    "threshold object without `threshold` key; skipped",
                                );
                                continue;
                            };
                            let delay = o
                                .get("delayAbortEval")
                                .and_then(|v| self.dur_field("delayAbortEval", v));
                            entries.push(ThresholdEntry::Detailed {
                                threshold: threshold.to_string(),
                                abort_on_fail: o
                                    .get("abortOnFail")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                                delay_abort_eval: delay,
                            });
                        }
                        _ => self.warn(
                            format!("thresholds.{metric}"),
                            "unsupported threshold entry; skipped",
                        ),
                    }
                }
                Some(ThresholdList::Many(entries))
            }
            _ => {
                self.warn(
                    format!("thresholds.{metric}"),
                    "unsupported threshold value; skipped",
                );
                None
            }
        }
    }

    fn scenario_from_k6(
        &mut self,
        name: &str,
        map: &serde_json::Map<String, serde_json::Value>,
    ) -> Scenario {
        let mut s = Scenario::default();
        for (key, value) in map {
            let path = format!("scenarios.{name}.{key}");
            match key.as_str() {
                "executor" => {
                    let raw = value.as_str().unwrap_or("");
                    s.executor = match raw {
                        "constant-vus" => ExecutorKind::ConstantVus,
                        "ramping-vus" => ExecutorKind::RampingVus,
                        "constant-arrival-rate" => ExecutorKind::ConstantArrivalRate,
                        "ramping-arrival-rate" => ExecutorKind::RampingArrivalRate,
                        "per-vu-iterations" => ExecutorKind::PerVuIterations,
                        "shared-iterations" => ExecutorKind::SharedIterations,
                        "externally-controlled" => ExecutorKind::ExternallyControlled,
                        other => {
                            self.warn(
                                path,
                                format!("unknown executor `{other}`; using constant-vus"),
                            );
                            ExecutorKind::ConstantVus
                        }
                    };
                }
                "vus" => s.vus = value.as_u64(),
                "duration" => s.duration = self.dur_field(&path, value),
                "iterations" => s.iterations = value.as_u64(),
                "startVUs" => s.start_vus = value.as_u64(),
                "stages" => {
                    if let Some(items) = value.as_array() {
                        s.stages = self.stages_from_json(&path, items);
                    }
                }
                "rate" => s.rate = value.as_f64(),
                "startRate" => s.start_rate = value.as_f64(),
                "timeUnit" => s.time_unit = self.dur_field(&path, value),
                "preAllocatedVUs" => s.pre_allocated_vus = value.as_u64(),
                "maxVUs" => s.max_vus = value.as_u64(),
                "maxDuration" => s.max_duration = self.dur_field(&path, value),
                "startTime" => s.start_time = self.dur_field(&path, value),
                "gracefulStop" => s.graceful_stop = self.dur_field(&path, value),
                "gracefulRampDown" => s.graceful_ramp_down = self.dur_field(&path, value),
                "exec" => s.exec = value.as_str().map(str::to_string),
                "tags" => {
                    if let Some(tags) = value.as_object() {
                        s.tags = tags
                            .iter()
                            .map(|(k, v)| (k.clone(), json_to_plain_string(v)))
                            .collect::<BTreeMap<_, _>>();
                    }
                }
                other => self.warn(
                    path,
                    format!("scenario option `{other}` is not supported; skipped"),
                ),
            }
        }
        s
    }

    fn stages_from_json(&mut self, path: &str, items: &[serde_json::Value]) -> Vec<Stage> {
        let mut out = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let duration = item.get("duration").and_then(|d| self.dur_field(path, d));
            let target = item.get("target").and_then(|t| t.as_f64());
            match (duration, target) {
                (Some(duration), Some(target)) => out.push(Stage { duration, target }),
                _ => self.warn(
                    format!("{path}[{i}]"),
                    "stage needs `duration` and `target`; skipped",
                ),
            }
        }
        out
    }

    fn dur_field(&mut self, path: &str, value: &serde_json::Value) -> Option<Dur> {
        match value {
            serde_json::Value::String(s) => match Dur::parse(s) {
                Ok(d) => Some(d),
                Err(e) => {
                    self.warn(path.to_string(), e);
                    None
                }
            },
            serde_json::Value::Number(n) => n
                .as_f64()
                .filter(|v| *v >= 0.0)
                .map(|secs| Dur::from(std::time::Duration::from_secs_f64(secs))),
            _ => {
                self.warn(path.to_string(), "expected a duration string; skipped");
                None
            }
        }
    }

    // -- function bodies -------------------------------------------------------

    fn convert_functions(&mut self) -> Result<(), ConvertError> {
        // Lifecycle functions are copied verbatim via js.script.
        if static_regex(r"export\s+function\s+(setup|teardown)\s*\(").is_match(&self.stripped) {
            self.has_lifecycle = true;
            self.warn(
                "setup()/teardown()",
                "lifecycle functions are copied into `js.script` verbatim; \
                 review them for k6 API usage",
            );
        }

        // Default function → flow for scenarios without `exec`.
        let default_body: Option<String> = self
            .find_function_body("default")
            .map(|(_, body)| body.to_string());
        let default_flow = default_body.map(|body| self.convert_body(&body));

        // Named exec functions.
        let execs: Vec<(String, String)> = self
            .plan
            .scenarios
            .iter()
            .filter_map(|(name, s)| s.exec.clone().map(|e| (name.clone(), e)))
            .collect();
        for (scenario_name, exec) in execs {
            let found: Option<String> = self
                .find_function_body(&exec)
                .map(|(_, body)| body.to_string());
            match found {
                Some(body) => {
                    let steps = self.convert_body(&body);
                    if let Some(s) = self.plan.scenarios.get_mut(&scenario_name) {
                        if steps.is_empty() {
                            // Nothing converted: fall back to running the JS.
                            self.needs_js_module = true;
                        } else {
                            s.flow = steps;
                            s.exec = None;
                        }
                    }
                }
                None => {
                    self.warn(
                        format!("scenarios.{scenario_name}.exec"),
                        format!(
                            "exported function `{exec}` was not found; the scenario \
                             will run it from `js.script` — review the conversion"
                        ),
                    );
                    self.needs_js_module = true;
                }
            }
        }

        if let Some(flow) = default_flow {
            if flow.is_empty() {
                self.needs_js_module = true;
                self.warn(
                    "export default function",
                    "no statements could be converted; the original script is kept \
                     in `js.script` and scenarios call it via `exec: default`",
                );
                for s in self.plan.scenarios.values_mut() {
                    if s.exec.is_none() && s.flow.is_empty() {
                        s.exec = Some("default".into());
                    }
                }
            } else {
                for s in self.plan.scenarios.values_mut() {
                    if s.exec.is_none() && s.flow.is_empty() {
                        s.flow = flow.clone();
                    }
                }
            }
        } else {
            // No default function: scenarios without exec cannot run.
            let orphans: Vec<String> = self
                .plan
                .scenarios
                .iter()
                .filter(|(_, s)| s.exec.is_none() && s.flow.is_empty())
                .map(|(n, _)| n.clone())
                .collect();
            if !orphans.is_empty() {
                return Err(ConvertError::Js(
                    "script has no `export default function` and no exec functions \
                     for its scenarios"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    /// Find `export default function ...{` or `export function NAME(...) {` and
    /// return (params, body without outer braces).
    fn find_function_body(&self, name: &str) -> Option<(String, &str)> {
        let re = if name == "default" {
            static_regex(r"export\s+default\s+function[^(]*\(([^)]*)\)")
        } else {
            // Build per-name regex; names are simple identifiers.
            return self.find_named_function_body(name);
        };
        let caps = re.captures(&self.stripped)?;
        let m = caps.get(0)?;
        let params = caps.get(1).map(|p| p.as_str().trim().to_string())?;
        let open = self.stripped[m.end()..].find('{')? + m.end();
        let close = match_delim(&self.stripped, open)?;
        Some((params, &self.stripped[open + 1..close]))
    }

    fn find_named_function_body(&self, name: &str) -> Option<(String, &str)> {
        let pattern = format!(
            r"(?:export\s+)?function\s+{}\s*\(([^)]*)\)",
            regex::escape(name)
        );
        let re = regex::Regex::new(&pattern).ok()?;
        let caps = re.captures(&self.stripped)?;
        let m = caps.get(0)?;
        let params = caps.get(1).map(|p| p.as_str().trim().to_string())?;
        let open = self.stripped[m.end()..].find('{')? + m.end();
        let close = match_delim(&self.stripped, open)?;
        Some((params, &self.stripped[open + 1..close]))
    }

    /// Convert a function body into flow steps, collecting unconverted lines.
    fn convert_body(&mut self, body: &str) -> Vec<Step> {
        let mut steps: Vec<Step> = Vec::new();
        let call_re = static_regex(
            r"\b(?:(http\.(?:get|post|put|del|patch|head|options|request))|(sleep)|(check)|(group))\s*\(",
        );
        let mut cursor = 0usize;
        while let Some(caps) = call_re.captures_at(body, cursor) {
            let Some(m) = caps.get(0) else { break };
            // Skip member accesses like `r.group(...)`.
            if m.start() > 0 && body.as_bytes()[m.start() - 1] == b'.' {
                cursor = m.start() + 1;
                continue;
            }
            // Skip matches inside string literals by checking quote balance.
            if inside_string(&body[..m.start()]) {
                cursor = m.end();
                continue;
            }
            self.process_leftover(&body[cursor..m.start()]);
            let open = m.end() - 1; // position of '('
            let Some(close) = match_delim(body, open) else {
                self.unconverted.push(body[m.start()..].trim().to_string());
                cursor = body.len();
                break;
            };
            let inner = &body[open + 1..close];
            if let Some(http) = caps.get(1) {
                let method_token = http.as_str().trim_start_matches("http.").to_string();
                self.convert_http_call(&method_token, inner, &mut steps);
            } else if caps.get(2).is_some() {
                self.convert_sleep(inner, &mut steps);
            } else if caps.get(3).is_some() {
                self.convert_check(inner, &mut steps);
            } else if caps.get(4).is_some() {
                let group_src = &body[open + 1..close];
                self.convert_group(group_src, &mut steps);
            }
            cursor = close + 1;
            // Swallow a trailing `;`.
            while body[cursor..].starts_with([';', ' ', '\t']) {
                cursor += 1;
            }
        }
        self.process_leftover(&body[cursor..]);
        steps
    }

    /// Record any non-trivial source between converted statements.
    fn process_leftover(&mut self, segment: &str) {
        // Drop the assignment/return that binds the upcoming converted call.
        let tail_re = static_regex(r"(?s)(?:(?:const|let|var)\s+)?[\w$]+\s*=\s*$|return\s*$");
        let segment = tail_re.replace(segment, "");
        for line in segment.lines() {
            let t = line.trim().trim_end_matches(';').trim();
            if t.is_empty() || t == ";" {
                continue;
            }
            self.unconverted.push(t.to_string());
        }
    }

    fn convert_http_call(&mut self, method_token: &str, inner: &str, steps: &mut Vec<Step>) {
        let mut args = split_top_level_args(inner);
        let method = if method_token == "request" {
            if args.is_empty() {
                self.warn("http.request", "missing arguments; skipped");
                return;
            }
            let m = args.remove(0);
            match string_literal(&m) {
                Some(s) => s.to_ascii_uppercase(),
                None => {
                    self.warn(
                        "http.request",
                        format!("dynamic method `{m}` is not supported; using GET"),
                    );
                    "GET".to_string()
                }
            }
        } else if method_token == "del" {
            "DELETE".to_string()
        } else {
            method_token.to_ascii_uppercase()
        };

        let Some(url_arg) = args.first().cloned() else {
            self.warn(
                format!("http.{method_token}"),
                "call without a URL; skipped",
            );
            return;
        };
        let Some(url) = self.url_from_arg(&url_arg) else {
            self.unconverted
                .push(format!("http.{method_token}({inner})"));
            self.warn(
                format!("http.{method_token}"),
                format!("URL argument `{url_arg}` is not a literal; statement kept in JS"),
            );
            return;
        };

        // k6 signatures: get/head/options(url, params); post/put/patch/del and
        // request(method, url, ...) take (url, body, params).
        let has_body = matches!(method_token, "post" | "put" | "patch" | "del" | "request");
        let body_idx = 1usize;
        let params_idx = if has_body { 2 } else { 1 };

        let mut req = RequestStep {
            name: Some(url_path(&url)),
            method: Some(method),
            url,
            ..Default::default()
        };

        if has_body {
            if let Some(body_arg) = args.get(body_idx) {
                self.body_from_arg(body_arg, &mut req);
            }
        }
        if let Some(params_arg) = args.get(params_idx) {
            self.params_from_arg(params_arg, &mut req);
        }

        steps.push(Step::Request(Box::new(req)));
    }

    /// Parse a URL argument: a string literal (any quote style) or a bare
    /// identifier (becomes a `${name}` template).
    fn url_from_arg(&mut self, arg: &str) -> Option<String> {
        let arg = arg.trim();
        if let Some(s) = string_literal(arg) {
            let url = s.replace("${__ENV.", "${env.");
            // Warn about complex template expressions.
            for cap in static_regex(r"\$\{([^}]*)\}").captures_iter(&url) {
                let expr = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let simple = static_regex(r"^[A-Za-z_$][\w$]*(?:\.[\w$]+)*$").is_match(expr);
                if !simple {
                    self.warn(
                        format!("url template `{expr}`"),
                        "template expression is not a simple identifier; review it",
                    );
                }
            }
            return Some(url);
        }
        if static_regex(r"^[A-Za-z_$][\w$]*$").is_match(arg) {
            self.warn(
                format!("url `{arg}`"),
                "URL is a JS variable; converted to a `${...}` template that must \
                 be set at runtime (extract or JS step)",
            );
            return Some(format!("${{{arg}}}"));
        }
        None
    }

    fn body_from_arg(&mut self, arg: &str, req: &mut RequestStep) {
        let arg = arg.trim();
        if arg.is_empty() || arg == "null" || arg == "undefined" {
            return;
        }
        if let Some(s) = string_literal(arg) {
            req.body = Some(Body::Text(s));
            return;
        }
        if let Some(rest) = arg.strip_prefix("JSON.stringify") {
            let rest = rest.trim();
            if rest.starts_with('(') {
                if let Some(close) = match_delim(rest, 0) {
                    let inner = rest[1..close].trim();
                    match js_object_to_json(inner) {
                        Ok(json) => {
                            req.body = Some(Body::Spec(BodySpec {
                                json: Some(json),
                                ..Default::default()
                            }));
                            return;
                        }
                        Err(e) => {
                            self.warn(
                                "JSON.stringify",
                                format!("body object could not be converted ({e}); body omitted"),
                            );
                            return;
                        }
                    }
                }
            }
        }
        self.warn(
            "request body",
            format!("body argument `{arg}` is not a literal; body omitted — review"),
        );
    }

    fn params_from_arg(&mut self, arg: &str, req: &mut RequestStep) {
        let arg = arg.trim();
        if arg.is_empty() || arg == "null" || arg == "undefined" {
            return;
        }
        let Ok(obj) = js_object_to_json(arg) else {
            self.warn(
                "request params",
                format!("params argument `{arg}` is not an object literal; ignored"),
            );
            return;
        };
        let Some(map) = obj.as_object() else {
            self.warn("request params", "params is not an object; ignored");
            return;
        };
        for (key, value) in map {
            match key.as_str() {
                "headers" => {
                    if let Some(headers) = value.as_object() {
                        req.headers = headers
                            .iter()
                            .map(|(k, v)| (k.clone(), json_to_plain_string(v)))
                            .collect::<IndexMap<_, _>>();
                    }
                }
                "tags" => {
                    if let Some(tags) = value.as_object() {
                        req.tags = tags
                            .iter()
                            .map(|(k, v)| (k.clone(), json_to_plain_string(v)))
                            .collect::<BTreeMap<_, _>>();
                    }
                }
                other => self.warn(
                    format!("params.{other}"),
                    "request param is not supported by the converter; skipped",
                ),
            }
        }
    }

    fn convert_sleep(&mut self, inner: &str, steps: &mut Vec<Step>) {
        let inner = inner.trim();
        if let Ok(secs) = inner.parse::<f64>() {
            if secs >= 0.0 {
                steps.push(Step::ThinkTime(ThinkTimeSpec::Constant {
                    duration: Dur::from(std::time::Duration::from_secs_f64(secs)),
                }));
                return;
            }
        }
        // Math.random() * a + b   |   b + Math.random() * a   |   Math.random() * a
        let re1 = static_regex(r"^Math\.random\(\)\s*\*\s*([\d.]+)\s*(?:\+\s*([\d.]+))?$");
        let re2 = static_regex(r"^([\d.]+)\s*\+\s*Math\.random\(\)\s*\*\s*([\d.]+)$");
        let parsed = re1
            .captures(inner)
            .map(|c| {
                let a: f64 = c
                    .get(1)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0.0);
                let b: f64 = c
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0.0);
                (a, b)
            })
            .or_else(|| {
                re2.captures(inner).map(|c| {
                    let b: f64 = c
                        .get(1)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0.0);
                    let a: f64 = c
                        .get(2)
                        .and_then(|m| m.as_str().parse().ok())
                        .unwrap_or(0.0);
                    (a, b)
                })
            });
        match parsed {
            Some((a, b)) => steps.push(Step::ThinkTime(ThinkTimeSpec::Uniform {
                min: Dur::from(std::time::Duration::from_secs_f64(b)),
                max: Dur::from(std::time::Duration::from_secs_f64(a + b)),
            })),
            None => self.warn(
                format!("sleep({inner})"),
                "sleep argument is not a number or `Math.random()*a+b`; skipped",
            ),
        }
    }

    fn convert_check(&mut self, inner: &str, steps: &mut [Step]) {
        let args = split_top_level_args(inner);
        if args.len() < 2 {
            self.warn("check()", "expected `check(res, { ... })`; skipped");
            return;
        }
        let res_ident = args[0].trim().to_string();
        let obj = args[1].trim();
        if !obj.starts_with('{') || !obj.ends_with('}') {
            self.warn("check()", "checks object is not a literal; skipped");
            return;
        }
        let entries = split_top_level_args(&obj[1..obj.len() - 1]);
        let mut conditions = Vec::new();
        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let Some((key, value)) = split_key_value(entry) else {
                self.warn(
                    "check()",
                    format!("cannot parse check entry `{entry}`; skipped"),
                );
                continue;
            };
            if let Some(cond) = self.condition_from_check(&key, &value, &res_ident) {
                conditions.push(cond);
            }
        }
        // Attach to the most recent request step.
        match steps.iter_mut().rev().find_map(|s| match s {
            Step::Request(r) => Some(r),
            _ => None,
        }) {
            Some(req) => req.checks.extend(conditions),
            None => self.warn(
                "check()",
                "no preceding request to attach checks to; checks dropped",
            ),
        }
    }

    fn condition_from_check(
        &mut self,
        name: &str,
        value: &str,
        res_ident: &str,
    ) -> Option<Condition> {
        // Unwrap `(r) => expr`, `r => expr`, `(r) => { return expr; }`.
        let arrow_re = static_regex(r"(?s)^\(?\s*([\w$]*)\s*\)?\s*=>\s*(.+)$");
        let (param, mut expr) = match arrow_re.captures(value.trim()) {
            Some(c) => (
                c.get(1).map(|m| m.as_str()).unwrap_or("").to_string(),
                c.get(2)
                    .map(|m| m.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            ),
            None => (res_ident.to_string(), value.trim().to_string()),
        };
        if expr.starts_with('{') {
            let ret_re = static_regex(r"(?s)^\{\s*return\s+(.*?);?\s*\}$");
            match ret_re.captures(&expr) {
                Some(c) => {
                    expr = c
                        .get(1)
                        .map(|m| m.as_str().trim())
                        .unwrap_or("")
                        .to_string()
                }
                None => {
                    self.warn(
                        format!("check `{name}`"),
                        "multi-statement check body; converted to a JS condition — review",
                    );
                }
            }
        }
        let p = if param.is_empty() { res_ident } else { &param };
        let pe = regex::escape(p);

        // r.status === 200 (possibly || chains)
        let eq_re = regex::Regex::new(&format!(r"^{pe}\.status\s*===?\s*(\d+)$")).ok()?;
        let parts: Vec<&str> = expr.split("||").map(str::trim).collect();
        if !parts.is_empty() && parts.iter().all(|part| eq_re.is_match(part)) {
            let codes: Vec<i64> = parts
                .iter()
                .filter_map(|part| eq_re.captures(part))
                .filter_map(|c| c.get(1))
                .filter_map(|m| m.as_str().parse().ok())
                .collect();
            return Some(if codes.len() == 1 {
                Condition::Status {
                    name: Some(name.to_string()),
                    equals: codes.first().copied(),
                    one_of: None,
                    matches: None,
                    on_failure: None,
                }
            } else {
                Condition::Status {
                    name: Some(name.to_string()),
                    equals: None,
                    one_of: Some(codes),
                    matches: None,
                    on_failure: None,
                }
            });
        }

        // r.body.includes('x')
        let inc_re = regex::Regex::new(&format!(r"(?s)^{pe}\.body\.includes\((.+)\)$")).ok()?;
        if let Some(c) = inc_re.captures(&expr) {
            if let Some(value) = c.get(1).and_then(|m| string_literal(m.as_str())) {
                return Some(Condition::BodyContains {
                    name: Some(name.to_string()),
                    value,
                    negate: false,
                    on_failure: None,
                });
            }
        }

        // r.timings.duration < N
        let dur_re = regex::Regex::new(&format!(
            r"^{pe}\.timings\.duration\s*<=?\s*(\d+(?:\.\d+)?)$"
        ))
        .ok()?;
        if let Some(c) = dur_re.captures(&expr) {
            let ms: f64 = c
                .get(1)
                .and_then(|m| m.as_str().parse().ok())
                .unwrap_or(0.0);
            return Some(Condition::Duration {
                name: Some(name.to_string()),
                max: Dur::from(std::time::Duration::from_secs_f64(ms / 1000.0)),
                on_failure: None,
            });
        }

        // Fallback: JS condition with the parameter renamed to `response`.
        let renamed = regex::Regex::new(&format!(r"\b{pe}\b"))
            .map(|re| re.replace_all(&expr, "response").into_owned())
            .unwrap_or_else(|_| expr.clone());
        self.warn(
            format!("check `{name}`"),
            "check body converted to a JS expression with the response bound to \
             `response`; verify the expression semantics",
        );
        Some(Condition::Js {
            name: Some(name.to_string()),
            expression: renamed,
            on_failure: None,
        })
    }

    fn convert_group(&mut self, call_src: &str, steps: &mut Vec<Step>) {
        // call_src is the inside of `group( ... )`.
        let args = split_top_level_args(call_src);
        let name = args
            .first()
            .and_then(|a| string_literal(a.trim()))
            .unwrap_or_else(|| "group".to_string());
        // Find the callback body: first '{' after `=>` or `function`.
        let cb_start = call_src
            .find("=>")
            .map(|i| i + 2)
            .or_else(|| call_src.find("function").map(|i| i + "function".len()));
        let inner_steps = match cb_start {
            Some(start) => match call_src[start..].find('{') {
                Some(rel) => {
                    let open = start + rel;
                    match match_delim(call_src, open) {
                        Some(close) => {
                            let body = call_src[open + 1..close].to_string();
                            self.convert_body(&body)
                        }
                        None => Vec::new(),
                    }
                }
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        if inner_steps.is_empty() {
            self.warn(
                format!("group('{name}')"),
                "group callback had no convertible statements",
            );
        }
        steps.push(Step::Group(GroupStep {
            name,
            steps: inner_steps,
        }));
    }

    // -- finishing ---------------------------------------------------------------

    fn finish(&mut self) {
        let unconverted = std::mem::take(&mut self.unconverted);
        if !unconverted.is_empty() {
            for line in &unconverted {
                self.warn(
                    "unconverted JS",
                    format!("`{line}` was not converted; kept in `js.script`"),
                );
            }
        }
        if !unconverted.is_empty() || self.has_lifecycle || self.needs_js_module {
            self.plan.js = Some(JsConfig {
                file: None,
                script: Some(stripped_script(&self.original)),
                timeout: None,
                memory_limit_mb: None,
            });
        }
    }
}

/// Copy of the original script with k6 import lines removed and a review header.
fn stripped_script(original: &str) -> String {
    let mut out = String::from(
        "// Converted from a k6 script by loadr-convert.\n\
         // k6 imports were removed: `http`, `check`, `sleep` and `group` are\n\
         // provided as loadr globals. Review before running.\n",
    );
    for line in original.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") && (trimmed.contains("'k6") || trimmed.contains("\"k6")) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------
// Lightweight JS source utilities
// ---------------------------------------------------------------------------

/// Remove `//` and `/* */` comments outside string literals.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            out.push(c as char);
            if c == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'\'' | b'"' | b'`' => {
                quote = Some(c);
                out.push(c as char);
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            _ => {
                // Preserve multi-byte UTF-8 sequences as raw bytes.
                let len = utf8_len(c);
                out.push_str(&src[i..(i + len).min(src.len())]);
                i += len;
            }
        }
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        b if b < 0x80 => 1,
        b if b >> 5 == 0b110 => 2,
        b if b >> 4 == 0b1110 => 3,
        _ => 4,
    }
}

/// Given `s[open]` is `(`, `{` or `[`, return the index of the matching closer,
/// skipping string literals.
fn match_delim(s: &str, open: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let open_ch = *bytes.get(open)?;
    let close_ch = match open_ch {
        b'(' => b')',
        b'{' => b'}',
        b'[' => b']',
        _ => return None,
    };
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else if c == b'\'' || c == b'"' || c == b'`' {
            quote = Some(c);
        } else if c == open_ch {
            depth += 1;
        } else if c == close_ch {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Split call arguments / object entries at top-level commas (string- and
/// bracket-aware).
fn split_top_level_args(s: &str) -> Vec<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else {
            match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => depth -= 1,
                b',' if depth == 0 => {
                    out.push(s[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

/// Split an object entry `key: value` at the first top-level colon.
fn split_key_value(entry: &str) -> Option<(String, String)> {
    let bytes = entry.as_bytes();
    let mut quote: Option<u8> = None;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
        } else {
            match c {
                b'\'' | b'"' | b'`' => quote = Some(c),
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => depth -= 1,
                b':' if depth == 0 => {
                    let raw_key = entry[..i].trim();
                    let key = string_literal(raw_key).unwrap_or_else(|| raw_key.to_string());
                    return Some((key, entry[i + 1..].trim().to_string()));
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Parse a JS string literal (single, double or backtick quotes) into its value.
fn string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    let first = s.chars().next()?;
    if !matches!(first, '\'' | '"' | '`') || s.len() < 2 || !s.ends_with(first) {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    // Reject strings with an unescaped instance of the quote inside (it's an
    // expression like 'a' + 'b', not a single literal).
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => return None,
            }
        } else if c == first {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Rough check: are we inside an unterminated string literal at this point?
fn inside_string(prefix: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = prefix.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == '\\' {
                    let _ = chars.next();
                } else if c == q {
                    quote = None;
                }
            }
            None => {
                if matches!(c, '\'' | '"' | '`') {
                    quote = Some(c);
                }
            }
        }
    }
    quote.is_some()
}

/// Extract the URL path from an absolute or relative URL (for request names).
fn url_path(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let path = match rest.find('/') {
        Some(i) => &rest[i..],
        None => "/",
    };
    let path = path.split('?').next().unwrap_or(path);
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn json_to_plain_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tolerant JS object literal → JSON
// ---------------------------------------------------------------------------

/// Convert a JS object/array literal to JSON: quotes unquoted keys, converts
/// single/backtick quotes, strips trailing commas, and maps `__ENV.X` to the
/// loadr template `"${env.X}"`.
fn js_object_to_json(src: &str) -> Result<serde_json::Value, String> {
    let tokens = tokenize_js_value(src)?;
    let mut out = String::with_capacity(src.len());
    for (i, tok) in tokens.iter().enumerate() {
        if tok == "," {
            if let Some(next) = tokens.get(i + 1) {
                if next == "}" || next == "]" {
                    continue; // trailing comma
                }
            } else {
                continue;
            }
        }
        out.push_str(tok);
    }
    serde_json::from_str(&out).map_err(|e| e.to_string())
}

fn tokenize_js_value(src: &str) -> Result<Vec<String>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '{' | '}' | '[' | ']' | ':' | ',' => {
                toks.push(c.to_string());
                i += 1;
            }
            '\'' | '"' | '`' => {
                let (value, next) = read_js_string(&chars, i)?;
                let normalized = value.replace("${__ENV.", "${env.");
                toks.push(
                    serde_json::to_string(&normalized)
                        .map_err(|e| format!("string encode: {e}"))?,
                );
                i = next;
            }
            '0'..='9' | '-' | '+' | '.' => {
                let start = i;
                i += 1;
                while i < chars.len() && matches!(chars[i], '0'..='9' | '.' | 'e' | 'E' | '+' | '-')
                {
                    i += 1;
                }
                toks.push(chars[start..i].iter().collect());
            }
            c if c.is_alphabetic() || c == '_' || c == '$' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '$' | '.'))
                {
                    i += 1;
                }
                let ident: String = chars[start..i].iter().collect();
                // Peek at the next significant character.
                let mut j = i;
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                let is_key = chars.get(j) == Some(&':');
                if matches!(ident.as_str(), "true" | "false" | "null") && !is_key {
                    toks.push(ident);
                } else if let Some(rest) = ident.strip_prefix("__ENV.") {
                    toks.push(
                        serde_json::to_string(&format!("${{env.{rest}}}"))
                            .map_err(|e| format!("string encode: {e}"))?,
                    );
                } else {
                    // Unquoted key, or an identifier value (kept as a template).
                    let rendered = if is_key {
                        ident
                    } else {
                        format!("${{{ident}}}")
                    };
                    toks.push(
                        serde_json::to_string(&rendered)
                            .map_err(|e| format!("string encode: {e}"))?,
                    );
                }
            }
            other => return Err(format!("unexpected `{other}` in object literal")),
        }
    }
    Ok(toks)
}

fn read_js_string(chars: &[char], start: usize) -> Result<(String, usize), String> {
    let quote = chars[start];
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            match chars.get(i + 1) {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(&other) => out.push(other),
                None => return Err("dangling escape in string".into()),
            }
            i += 2;
            continue;
        }
        if c == quote {
            return Ok((out, i + 1));
        }
        out.push(c);
        i += 1;
    }
    Err("unterminated string literal".into())
}

fn static_regex(pattern: &'static str) -> &'static regex::Regex {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<&'static str, &'static regex::Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(re) = guard.get(pattern) {
        return re;
    }
    let compiled: &'static regex::Regex = Box::leak(Box::new(
        regex::Regex::new(pattern).unwrap_or_else(|_| unreachable!("static regex `{pattern}`")),
    ));
    guard.insert(pattern, compiled);
    compiled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_object_tolerates_js_syntax() {
        let v =
            js_object_to_json("{ vus: 5, duration: '30s', nested: { list: [1, 2,], ok: true, }, }")
                .expect("parse");
        assert_eq!(v["vus"], serde_json::json!(5));
        assert_eq!(v["duration"], serde_json::json!("30s"));
        assert_eq!(v["nested"]["list"], serde_json::json!([1, 2]));
        assert_eq!(v["nested"]["ok"], serde_json::json!(true));
    }

    #[test]
    fn js_object_converts_env_refs() {
        let v = js_object_to_json("{ host: __ENV.HOST, url: `${__ENV.BASE}/x` }").expect("parse");
        assert_eq!(v["host"], serde_json::json!("${env.HOST}"));
        assert_eq!(v["url"], serde_json::json!("${env.BASE}/x"));
    }

    #[test]
    fn string_literal_handles_quotes_and_escapes() {
        assert_eq!(string_literal("'a b'").as_deref(), Some("a b"));
        assert_eq!(string_literal("\"x\\\"y\"").as_deref(), Some("x\"y"));
        assert_eq!(string_literal("`t ${v}`").as_deref(), Some("t ${v}"));
        assert_eq!(string_literal("'a' + 'b'"), None);
        assert_eq!(string_literal("ident"), None);
    }

    #[test]
    fn match_delim_skips_strings() {
        let s = "fn(a, 'b)c', { d: ')' })";
        assert_eq!(match_delim(s, 2), Some(s.len() - 1));
    }

    #[test]
    fn split_args_is_bracket_and_string_aware() {
        let args = split_top_level_args("url, JSON.stringify({a: 1, b: 2}), { h: ',' }");
        assert_eq!(args.len(), 3);
        assert_eq!(args[0], "url");
        assert_eq!(args[1], "JSON.stringify({a: 1, b: 2})");
    }

    #[test]
    fn strip_comments_preserves_strings() {
        let out = strip_comments("const u = 'http://x'; // note\n/* gone */const v = 1;");
        assert!(out.contains("'http://x'"));
        assert!(!out.contains("note"));
        assert!(!out.contains("gone"));
        assert!(out.contains("const v = 1;"));
    }

    #[test]
    fn sleep_variants() {
        let mut cv = K6Converter::default();
        let mut steps = Vec::new();
        cv.convert_sleep("3", &mut steps);
        cv.convert_sleep("Math.random() * 4 + 2", &mut steps);
        cv.convert_sleep("Math.random() * 5", &mut steps);
        assert_eq!(steps.len(), 3);
        match &steps[1] {
            Step::ThinkTime(ThinkTimeSpec::Uniform { min, max }) => {
                assert_eq!(*min, Dur::from_secs(2));
                assert_eq!(*max, Dur::from_secs(6));
            }
            other => panic!("expected uniform, got {other:?}"),
        }
        match &steps[2] {
            Step::ThinkTime(ThinkTimeSpec::Uniform { min, max }) => {
                assert_eq!(*min, Dur::from_secs(0));
                assert_eq!(*max, Dur::from_secs(5));
            }
            other => panic!("expected uniform, got {other:?}"),
        }
        cv.convert_sleep("someVariable", &mut steps);
        assert_eq!(steps.len(), 3, "unparseable sleep should be skipped");
        assert!(cv.warnings.iter().any(|w| w.element.contains("sleep")));
    }
}
