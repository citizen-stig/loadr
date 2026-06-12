# Browser

The `browser` protocol drives a **real headless Chrome** over the Chrome
DevTools Protocol (CDP). A `request` navigates the page to a URL, waits for the
load to settle, then reads **Navigation Timing** and **Web Vitals** straight out
of the page — so the numbers reflect what a user's browser actually does:
DNS/connect/TLS, time to first byte, the `DOMContentLoaded` and `load` events,
first and largest contentful paint, and every subresource the page pulls in.

```yaml
plugins:
  - name: browser            # register the browser protocol

scenarios:
  homepage:
    executor: constant-vus
    vus: 5
    duration: 1m
    flow:
      - request:
          name: load homepage
          protocol: browser   # required — there is no URL-scheme shorthand
          url: https://example.com
          timeout: 30s        # navigation timeout (default 30s)
          checks:
            - { type: status, equals: 200 }
            - { type: body_contains, value: "</html>" }
```

## When to use it

Use `browser` when you need **real client-side timing** — paint metrics,
JavaScript execution, and the cost of all the subresources a page fetches. Use
the protocol-level [`http`](http.md) client for everything else: it is far
cheaper per request and measures the transport precisely, but it does not render
a page, run scripts, or fetch subresources.

## Runtime requirement

A Chrome/Chromium binary must be installed on the runner (the handler launches
`/usr/bin/google-chrome` with `--headless=new --no-sandbox --disable-gpu
--disable-dev-shm-usage`). Chrome is launched **lazily** — only on the first
browser request — so tests that never reach a browser step pay nothing.

One Chrome process is shared per run. Each VU gets its **own tab**, reused across
requests, so navigation within a VU keeps a warm cache and a single browsing
session (a VU models one user). Navigation failures (DNS, connection refused,
aborts) are recorded as a failed sample with `status = 0` and an `error`, not as
a crash; only a timeout aborts the step.

## Request shape

| Field | Meaning |
|---|---|
| `protocol: browser` | Required. The browser protocol has no URL-scheme alias, so it must be named explicitly and listed under `plugins:`. |
| `url` | Absolute URL to navigate to (`http://` or `https://`), passed verbatim to the page. Supports `${...}`. |
| `timeout` | Navigation timeout; falls back to `defaults.http.timeout`, then 30s. |
| `checks` / `assert` | Run against the navigation: `status` (the real HTTP status of the main document), `body_contains` / `body_matches` (the rendered HTML), `duration`, etc. |

Only the navigation timeout is taken from `defaults.http`; other HTTP options
(TLS, redirects, compression, cookies) do not apply to the browser protocol.

## Metrics

Browser navigations record into the generic `plugin_*` metric family, plus the
shared failure and byte counters:

| Metric | Kind | Meaning |
|---|---|---|
| `plugin_reqs` | Counter | navigations |
| `plugin_req_duration` | Trend | full navigation time (ms) |
| `http_req_failed` | Rate | navigation error or status ≥ 400 |
| `data_received` | Counter | bytes transferred for the document + subresources |

The standard sample tags apply (`name`, `method`, `status`, `proto = browser`,
`scenario`, `group`).

### Web Vitals & timing extras

Each response carries the captured page metrics in `extras`, available to `js`
conditions and JavaScript steps via `response.extras`:

| Key | Meaning |
|---|---|
| `fcp_ms` | First Contentful Paint (may be `null` if unavailable) |
| `lcp_ms` | Largest Contentful Paint (captured via `PerformanceObserver`; may be `null`) |
| `dcl_ms` | `DOMContentLoaded` event end |
| `load_ms` | `load` event end |
| `resources` | number of subresources fetched |
| `transferred_bytes` | total transfer size (document + subresources) |
| `title` | the page's `document.title` |

The Navigation Timing phases (DNS, connect, TLS, TTFB, receiving, total
duration) are mapped onto loadr's standard request timings, so they appear in
the trend breakdown alongside other protocols.
