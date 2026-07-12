"use strict";
// loadr JS prelude. Builds the script-facing standard library on top of the
// native __loadr_* functions registered from Rust. Runs once per VU context,
// before the user's module is evaluated.
(function () {
  // ----- console ------------------------------------------------------------
  const fmt = (args) =>
    args
      .map((x) => {
        if (typeof x === "string") return x;
        if (x === undefined) return "undefined";
        try {
          const s = JSON.stringify(x);
          return s === undefined ? String(x) : s;
        } catch (e) {
          return String(x);
        }
      })
      .join(" ");
  globalThis.console = {
    log: (...a) => __loadr_log("info", fmt(a)),
    info: (...a) => __loadr_log("info", fmt(a)),
    warn: (...a) => __loadr_log("warn", fmt(a)),
    error: (...a) => __loadr_log("error", fmt(a)),
    debug: (...a) => __loadr_log("debug", fmt(a)),
  };

  // ----- core scripting helpers ---------------------------------------------
  globalThis.sleep = (seconds) => __loadr_sleep(Number(seconds));

  globalThis.check = function (val, checks, _tags) {
    let all = true;
    if (checks) {
      for (const key of Object.keys(checks)) {
        const c = checks[key];
        let pass;
        if (typeof c === "function") {
          try {
            pass = !!c(val);
          } catch (e) {
            pass = false;
          }
        } else {
          pass = !!c;
        }
        __loadr_check(String(key), pass);
        if (!pass) all = false;
      }
    }
    return all;
  };

  globalThis.group = function (name, fn) {
    __loadr_group_push(String(name));
    try {
      return fn();
    } finally {
      __loadr_group_pop();
    }
  };

  // ----- custom metrics -------------------------------------------------------
  const makeMetric = (kind) => {
    function Metric(name) {
      if (!(this instanceof Metric)) return new Metric(name);
      this.name = String(name);
      this.kind = kind;
    }
    Metric.prototype.add = function (value, tags) {
      const v = typeof value === "boolean" ? (value ? 1 : 0) : Number(value);
      __loadr_metric_add(this.name, kind, v, tags === null ? undefined : tags);
      return this;
    };
    return Metric;
  };
  globalThis.Counter = makeMetric("counter");
  globalThis.Gauge = makeMetric("gauge");
  globalThis.Rate = makeMetric("rate");
  globalThis.Trend = makeMetric("trend");

  // ----- http -----------------------------------------------------------------
  const http = {
    request(method, url, body, params) {
      const p = params || {};
      const headers = Object.assign({}, p.headers);
      let payload = body;
      if (
        payload !== undefined &&
        payload !== null &&
        typeof payload !== "string" &&
        !(payload instanceof ArrayBuffer) &&
        !ArrayBuffer.isView(payload)
      ) {
        payload = JSON.stringify(payload);
        if (!Object.keys(headers).some((k) => k.toLowerCase() === "content-type")) {
          headers["Content-Type"] = "application/json";
        }
      }
      const resp = __loadr_http(
        String(method).toUpperCase(),
        String(url),
        payload === undefined ? null : payload,
        { headers, timeout: p.timeout, tags: p.tags, name: p.name }
      );
      resp.json = function () {
        return JSON.parse(this.body);
      };
      return resp;
    },
    get(url, params) {
      return this.request("GET", url, null, params);
    },
    head(url, params) {
      return this.request("HEAD", url, null, params);
    },
    options(url, params) {
      return this.request("OPTIONS", url, null, params);
    },
    post(url, body, params) {
      return this.request("POST", url, body, params);
    },
    put(url, body, params) {
      return this.request("PUT", url, body, params);
    },
    patch(url, body, params) {
      return this.request("PATCH", url, body, params);
    },
    del(url, body, params) {
      return this.request("DELETE", url, body, params);
    },
  };
  globalThis.http = http;

  // ----- crypto / encoding -----------------------------------------------------
  globalThis.crypto = {
    sha256: (data, enc) => __loadr_digest("sha256", data, enc),
    sha384: (data, enc) => __loadr_digest("sha384", data, enc),
    sha512: (data, enc) => __loadr_digest("sha512", data, enc),
    sha1: (data, enc) => __loadr_digest("sha1", data, enc),
    md5: (data, enc) => __loadr_digest("md5", data, enc),
    hmac: (algo, secret, data, enc) => __loadr_hmac(String(algo), secret, data, enc),
    randomBytes: (n) => Array.from(__loadr_random_bytes(n)),
    uuidv4: () => __loadr_uuidv4(),
  };

  globalThis.encoding = {
    b64encode: (data, variant) => __loadr_b64encode(data, variant),
    b64decode: (data, variant) => __loadr_b64decode(String(data), variant),
  };

  // ----- environment, files ------------------------------------------------------
  globalThis.__ENV = new Proxy(
    {},
    {
      get: (_t, p) => (typeof p === "string" ? __loadr_env(p) : undefined),
      has: (_t, p) => typeof p === "string" && __loadr_env(p) !== undefined,
    }
  );

  globalThis.open = (path, mode) => __loadr_open(String(path), mode);

  // ----- session --------------------------------------------------------------
  const vars = new Proxy(
    {},
    {
      get: (_t, p) => (typeof p === "string" ? __loadr_get_var(p) : undefined),
      set: (_t, p, v) => {
        if (typeof p === "string") __loadr_set_var(p, v === undefined ? null : v);
        return true;
      },
      has: (_t, p) => typeof p === "string" && __loadr_get_var(p) !== undefined,
      deleteProperty: (_t, p) => {
        if (typeof p === "string") __loadr_set_var(p, null);
        return true;
      },
    }
  );

  globalThis.session = {
    vars,
    get vu() {
      return __loadr_vu_info().vu;
    },
    get iteration() {
      return __loadr_vu_info().iteration;
    },
    get scenario() {
      return __loadr_vu_info().scenario;
    },
    data: (source) => __loadr_data_row(String(source)),
    cookieGet: (url, name) => __loadr_cookie_get(String(url), String(name)),
    cookieSet: (url, name, value) => __loadr_cookie_set(String(url), String(name), String(value)),
    cookiesClear: () => __loadr_cookies_clear(),
    counterAdd: (name, value, tags) =>
      __loadr_metric_add(String(name), "counter", Number(value), tags === null ? undefined : tags),
    gaugeSet: (name, value, tags) =>
      __loadr_metric_add(String(name), "gauge", Number(value), tags === null ? undefined : tags),
    rateAdd: (name, value, tags) =>
      __loadr_metric_add(
        String(name),
        "rate",
        typeof value === "boolean" ? (value ? 1 : 0) : Number(value),
        tags === null ? undefined : tags
      ),
    trendAdd: (name, value, tags) =>
      __loadr_metric_add(String(name), "trend", Number(value), tags === null ? undefined : tags),
  };

  // Used by VuScript::eval to expose the YAML step's `response` binding.
  globalThis.__loadr_eval_response = function () {
    try {
      return __loadr_get_var("response");
    } catch (e) {
      return undefined;
    }
  };
})();
