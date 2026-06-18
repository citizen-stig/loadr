#!/usr/bin/env python3
"""Generate the /plugins index + a detail page per plugin from PLUGINS data.

Single source of truth for the plugins section. Writes:
  site/plugins/index.html              — compact, grouped card index
  site/plugins/<slug>/index.html       — full detail page per plugin

Adding a plugin = append one dict to PLUGINS below and re-run:
    python3 site/build-plugins.py

Output is committed (so Tailwind's content scan sees the classes) and copied
to dist/plugins/ by deploy.sh. The shared nav is injected later via the
<!-- INCLUDE:NAV --> marker by build-nav.py.
"""
import pathlib

# ---------------------------------------------------------------------------
# Data — one record per plugin. `description`, `tagline` and `yaml` may contain
# HTML; entities (&lt; &gt;) are pre-escaped. Order defines prev/next + grouping.
# ---------------------------------------------------------------------------
PLUGINS = [
    {
        "slug": "mongo",
        "name": "MongoDB",
        "category": "Database",
        "schemes": ["mongodb://", "mongo://"],
        "tagline": "Insert, find, update, aggregate and command — one operation per request, on the official Mongo driver.",
        "description": (
            'Drive MongoDB directly: <code class="text-flare">insert</code>, <code class="text-flare">find</code>, '
            '<code class="text-flare">update</code>, <code class="text-flare">delete</code>, '
            '<code class="text-flare">aggregate</code> and <code class="text-flare">command</code> — one operation per '
            'request. Ships the heavy <code class="text-flare">mongodb</code> Rust driver on its own.'
        ),
        "yaml_file": "mongo.yaml",
        "yaml": """plugins:
  - name: mongo          # resolves mongodb:// at runtime

scenarios:
  writes:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          url: mongodb://loadr:loadr@db:27017/loadr
          plugin:
            operation: insert
            collection: products
            document: { name: "vu-${vu}-item", price: 12.5 }
          assert:
            - { type: status, equals: 1 }   # 1 = ok, 0 = error

thresholds:
  mongo_req_duration: [ "p(95)&lt;300ms" ]""",
        "metrics": ["mongo_reqs", "mongo_req_duration", "mongo_docs"],
        "docs": "/docs/plugins/mongo.html",
        "video": "15-mongo-plugin",
        "demo_num": 15,
        "demo_blurb": "Install the plugin, then run a real insert / find / aggregate workload against MongoDB",
    },
    {
        "slug": "postgres",
        "name": "PostgreSQL",
        "category": "Database",
        "schemes": ["postgres://", "postgresql://"],
        "tagline": "Parameterised SELECT / INSERT / UPDATE bound with $1, $2 … and pooled per VU via sqlx.",
        "description": (
            'The query <em>is</em> the request. Parameterised <code class="text-flare">SELECT</code>/<code class="text-flare">INSERT</code>/<code class="text-flare">UPDATE</code>, '
            'bound safely with <code class="text-flare">$1, $2, …</code> and pooled per VU via <code class="text-flare">sqlx</code>. '
            'Built with only the <code class="text-flare">postgres</code> feature — no <code class="text-flare">rsa</code> advisory.'
        ),
        "yaml_file": "postgres.yaml",
        "yaml": """plugins:
  - name: postgres       # resolves postgres:// at runtime

scenarios:
  reads:
    executor: constant-vus
    vus: 10
    duration: 30s
    flow:
      - request:
          url: postgres://loadr:loadr@db:5432/loadr
          sql:
            query: SELECT id, name FROM products WHERE price &lt; $1
            params: ["50"]
          checks:
            - { type: status, equals: 1 }
            - { type: duration, max: 250ms }

thresholds:
  postgres_req_duration: [ "p(95)&lt;300ms" ]
  postgres_reqs: [ "count&gt;0" ]""",
        "metrics": ["postgres_reqs", "postgres_req_duration", "postgres_rows"],
        "docs": "/docs/plugins/postgres.html",
        "video": "16-postgres-plugin",
        "demo_num": 16,
        "demo_blurb": "Install the plugin, then drive parameterised queries against PostgreSQL",
    },
    {
        "slug": "mysql",
        "name": "MySQL",
        "category": "Database",
        "schemes": ["mysql://"],
        "tagline": "One query per request over the MySQL wire protocol, safely bound and pooled per VU.",
        "description": (
            'Drive MySQL over its wire protocol: one query per request, with <code class="text-flare">?</code> placeholders bound '
            'safely by the driver and a connection pool per VU via <code class="text-flare">sqlx</code> (only the '
            '<code class="text-flare">mysql</code> feature).'
        ),
        "yaml_file": "mysql.yaml",
        "yaml": """plugins:
  - name: mysql          # resolves mysql:// at runtime

scenarios:
  reads:
    executor: constant-arrival-rate
    rate: 100
    duration: 30s
    pre_allocated_vus: 10
    max_vus: 40
    flow:
      - request:
          url: mysql://loadr:loadr@db:3306/loadr
          sql:
            query: SELECT COUNT(*) AS n FROM products WHERE stock &gt; ?
            params: ["0"]
          checks:
            - { type: status, equals: 1 }

thresholds:
  mysql_req_duration: [ "p(95)&lt;300ms" ]
  mysql_reqs: [ "count&gt;0" ]""",
        "metrics": ["mysql_reqs", "mysql_req_duration", "mysql_rows"],
        "docs": "/docs/plugins/mysql.html",
        "video": "17-mysql-plugin",
        "demo_num": 17,
        "demo_blurb": "Install the plugin, then run a steady query workload against MySQL",
    },
    {
        "slug": "redis",
        "name": "Redis",
        "category": "Cache",
        "schemes": ["redis://", "rediss://"],
        "tagline": "Speak RESP over TCP — SET, GET, INCR, EXPIRE and the rest, one command per request.",
        "description": (
            'Speak RESP directly over TCP — one command per request in argv form: '
            '<code class="text-flare">SET</code>, <code class="text-flare">GET</code>, <code class="text-flare">INCR</code>, '
            '<code class="text-flare">EXPIRE</code>, <code class="text-flare">PING</code> and the rest. Pure Rust — no '
            '<code class="text-flare">redis</code> crate quirks, no OpenSSL.'
        ),
        "yaml_file": "redis.yaml",
        "yaml": """plugins:
  - name: redis          # resolves redis:// at runtime

scenarios:
  cache_churn:
    executor: constant-vus
    vus: 20
    duration: 15s
    flow:
      - request:
          url: redis://cache.example.com:6379
          plugin:
            command: ["SET", "session:${vu}", "active"]
          checks:
            - { type: status, equals: 0 }   # 0 = +OK, 1 = RESP error
            - { type: body_contains, value: OK }

thresholds:
  redis_req_duration: [ "p(95)&lt;100ms" ]""",
        "metrics": ["redis_reqs", "redis_req_duration"],
        "docs": "/docs/plugins/redis.html",
        "video": "18-redis-plugin",
        "demo_num": 18,
        "demo_blurb": "Install the plugin, then hammer SET / GET / INCR against Redis over RESP",
    },
    {
        "slug": "kafka",
        "name": "Apache Kafka",
        "category": "Messaging",
        "schemes": ["kafka://"],
        "tagline": "Produce and fetch against a broker, one operation per request — pure-Rust, no librdkafka.",
        "description": (
            'Produce and fetch against Kafka with one operation per request. The broker is the URL authority and the topic '
            'its path (<code class="text-flare">kafka://broker:9092/topic</code>). Ships the pure-Rust '
            '<code class="text-flare">rskafka</code> client — no librdkafka, no C toolchain.'
        ),
        "yaml_file": "kafka.yaml",
        "yaml": """plugins:
  - name: kafka          # resolves kafka:// at runtime

scenarios:
  producers:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          url: kafka://broker.example.com:9092/loadr-demo
          plugin:
            operation: produce
            key: "vu-${vu}"
            value: "event from vu ${vu} iter ${iteration}"
          assert:
            - { type: status, equals: 1 }   # 1 = ok, 0 = client error

thresholds:
  kafka_req_duration: [ "p(95)&lt;1s" ]
  kafka_reqs: [ "count&gt;0" ]""",
        "metrics": ["kafka_reqs", "kafka_req_duration", "kafka_msgs"],
        "docs": "/docs/plugins/kafka.html",
        "video": "19-kafka-plugin",
        "demo_num": 19,
        "demo_blurb": "Install the plugin, then produce and fetch messages against Apache Kafka",
    },
    {
        "slug": "rabbitmq",
        "name": "RabbitMQ",
        "category": "Messaging",
        "schemes": ["amqp://", "amqps://", "rabbitmq://"],
        "tagline": "Publish and consume over AMQP 0.9.1, declaring queues inline — the pure-Rust lapin client.",
        "description": (
            'Publish and get against an AMQP 0.9.1 broker, one operation per request — declare queues inline, ack on '
            'consume. Ships the pure-Rust <code class="text-flare">lapin</code> client on its own.'
        ),
        "yaml_file": "rabbitmq.yaml",
        "yaml": """plugins:
  - name: rabbitmq       # resolves amqp:// at runtime

scenarios:
  publish:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          url: amqp://loadr:loadr@broker.example.com:5672/%2f
          plugin:
            operation: publish
            routing_key: loadr.work
            queue: loadr.work
            declare_queue: true
            body: '{"vu": ${vu}, "iteration": ${iteration}}'
          assert:
            - { type: status, equals: 1 }   # 1 = ok, 0 = broker error

thresholds:
  rabbitmq_req_duration: [ "p(95)&lt;300ms" ]
  rabbitmq_reqs: [ "count&gt;0" ]""",
        "metrics": ["rabbitmq_reqs", "rabbitmq_req_duration", "rabbitmq_msgs"],
        "docs": "/docs/plugins/rabbitmq.html",
        "video": "20-rabbitmq-plugin",
        "demo_num": 20,
        "demo_blurb": "Install the plugin, then publish and drain a queue against RabbitMQ over AMQP",
    },
    {
        "slug": "elasticsearch",
        "name": "Elasticsearch",
        "category": "Search",
        "schemes": ["elasticsearch://", "es://"],
        "tagline": "Index, get, search and bulk over the HTTP/JSON API — on loadr's own hyper stack.",
        "description": (
            'Drive Elasticsearch over its HTTP/JSON API: <code class="text-flare">index</code>, <code class="text-flare">get</code>, '
            '<code class="text-flare">search</code> and <code class="text-flare">bulk</code> — one operation per request. '
            'Talks over loadr\'s own <code class="text-flare">hyper</code> + <code class="text-flare">hyper-rustls</code> stack — '
            'no heavy official client, no system OpenSSL.'
        ),
        "yaml_file": "elasticsearch.yaml",
        "yaml": """plugins:
  - name: elasticsearch  # resolves elasticsearch:// at runtime

scenarios:
  writes:
    executor: constant-vus
    vus: 5
    duration: 15s
    flow:
      - request:
          url: elasticsearch://es.example.com:9200
          plugin:
            operation: index
            index: products
            document:
              name: "vu-${vu}-item"
              price: 12.5
              stock: 3
          assert:
            - { type: status, equals: 1 }   # 1 = ok, 0 = error

thresholds:
  elasticsearch_req_duration: [ "p(95)&lt;400ms" ]
  elasticsearch_reqs: [ "count&gt;0" ]""",
        "metrics": ["elasticsearch_reqs", "elasticsearch_req_duration", "elasticsearch_docs"],
        "docs": "/docs/plugins/elasticsearch.html",
        "video": "21-elasticsearch-plugin",
        "demo_num": 21,
        "demo_blurb": "Install the plugin, then index, bulk-load and search against Elasticsearch over HTTP/JSON",
    },
]

CATEGORY_ORDER = ["Database", "Cache", "Messaging", "Search"]
CATEGORY_BLURB = {
    "Database": "Relational and document stores — the query (or operation) is the request.",
    "Cache": "Key/value and in-memory stores driven at their native wire protocol.",
    "Messaging": "Brokers and queues — produce, publish, fetch and consume.",
    "Search": "Search and analytics engines over their HTTP/JSON APIs.",
}


# ---------------------------------------------------------------------------
# Rendering helpers
# ---------------------------------------------------------------------------
def head(title, desc, canonical):
    return f"""<!doctype html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<!-- Google Analytics (gtag.js) loaded via Consent Mode; see /assets/consent.js -->
<script src="/assets/consent.js"></script>
<title>{title}</title>
<meta name="description" content="{desc}">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:url" content="{canonical}">
<meta property="og:type" content="website">
<link rel="canonical" href="{canonical}">
<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/site.css">
</head>
<body class="antialiased overflow-x-clip">

<!-- INCLUDE:NAV -->
"""

FOOTER = """
</main>

<footer class="border-t border-edge/60 bg-coal/40 py-14">
  <div class="mx-auto max-w-7xl px-5">
    <div class="grid gap-10 md:grid-cols-4">
      <div>
        <div class="flex items-center gap-2">
          <svg width="20" height="20" viewBox="0 0 32 32"><path d="M18 2 L8 18 L15 18 L13 30 L24 13 L17 13 Z" fill="#ef4444"/></svg>
          <span class="font-extrabold text-white">loadr<span class="text-ember">.io</span></span>
        </div>
        <p class="mt-3 text-sm text-smoke">Break things on purpose.<br>© 2026 loadr. All rights reserved.</p>
      </div>
      <div>
        <h4 class="text-xs font-bold uppercase tracking-wider text-smoke">Product</h4>
        <ul class="mt-3 space-y-2 text-sm text-smoke">
          <li><a class="hover:text-flare" href="/#features">Features</a></li>
          <li><a class="hover:text-flare" href="/#protocols">Protocols</a></li>
          <li><a class="hover:text-flare" href="/#distributed">Distributed</a></li>
          <li><a class="hover:text-flare" href="/#webui">Web UI</a></li>
          <li><a class="hover:text-flare" href="/plugins/">Plugins</a></li>
          <li><a class="hover:text-flare" href="/download/">Download</a></li>
        </ul>
      </div>
      <div>
        <h4 class="text-xs font-bold uppercase tracking-wider text-smoke">Documentation</h4>
        <ul class="mt-3 space-y-2 text-sm text-smoke">
          <li><a class="hover:text-flare" href="/docs/getting-started/first-test.html">Your first test</a></li>
          <li><a class="hover:text-flare" href="/docs/yaml/overview.html">YAML reference</a></li>
          <li><a class="hover:text-flare" href="/docs/js/api.html">JS API</a></li>
          <li><a class="hover:text-flare" href="/docs/plugins/developing.html">Plugin development</a></li>
          <li><a class="hover:text-flare" href="/docs/credits.html">Credits &amp; influences</a></li>
        </ul>
      </div>
      <div>
        <h4 class="text-xs font-bold uppercase tracking-wider text-smoke">Source</h4>
        <ul class="mt-3 space-y-2 text-sm text-smoke">
          <li><a class="hover:text-flare" href="/download/">Download</a></li>
          <li><a class="hover:text-flare" href="https://github.com/levantar-ai/loadr" rel="noopener">Source (GitHub)</a></li>
          <li><a class="hover:text-flare" href="https://github.com/levantar-ai/loadr/releases" rel="noopener">Releases</a></li>
          <li><a class="hover:text-flare" href="https://github.com/levantar-ai/loadr/blob/main/LICENSE" rel="noopener">License (Elastic 2.0)</a></li>
          <li><a class="hover:text-flare" href="/examples/">Examples</a></li>
          <li><a class="hover:text-flare" href="/docs/">Documentation</a></li>
        </ul>
      </div>
    </div>
    <div class="mt-12 border-t border-edge/60 pt-6 text-xs text-smoke">
      © 2026 loadr. Built in Rust. The loadr tool ships zero telemetry — it never phones home. This site uses privacy-first, consent-based analytics only. · <a class="hover:text-flare" href="/privacy/">Privacy</a> · <a class="hover:text-flare" href="/cookies/">Cookies</a>
    </div>
  </div>
</footer>

<script src="/assets/site.js" defer></script>
</body>
</html>
"""


def scheme_badges(schemes, gap="gap-2"):
    out = []
    for i, s in enumerate(schemes):
        cls = "text-flare" if i == 0 else "text-smoke"
        out.append(f'<code class="rounded-full border border-edge bg-coal px-3 py-1 font-mono text-xs {cls}">{s}</code>')
    return f'<div class="flex flex-wrap items-center {gap}">' + "".join(out) + "</div>"


def metrics_line(metrics):
    return " · ".join(f'<code class="text-flare">{m}</code>' for m in metrics)


def monogram(p, size="h-10 w-10 text-lg"):
    return (f'<span class="flex {size} shrink-0 items-center justify-center rounded-lg '
            f'border border-edge bg-coal font-mono font-bold text-flare">{p["name"][0]}</span>')


def card(p):
    return (
        f'<a href="/plugins/{p["slug"]}/" class="group flex flex-col rounded-2xl border border-edge bg-panel p-5 transition hover:border-ember/60 hover:bg-coal/40">'
        '<div class="flex items-center gap-3">'
        + monogram(p) +
        '<div class="min-w-0">'
        f'<h3 class="font-bold text-white">{p["name"]}</h3>'
        f'<code class="font-mono text-xs text-smoke">{p["schemes"][0]}</code>'
        '</div></div>'
        f'<p class="mt-3 flex-1 text-sm text-smoke">{p["tagline"]}</p>'
        '<div class="mt-4 flex items-center justify-between">'
        f'<span class="rounded-full border border-edge bg-coal px-2.5 py-0.5 text-xs text-ash">{p["category"]}</span>'
        '<span class="text-sm font-semibold text-flare group-hover:underline">View plugin →</span>'
        '</div></a>'
    )


def render_index():
    cards_by_cat = {}
    for p in PLUGINS:
        cards_by_cat.setdefault(p["category"], []).append(p)

    groups = []
    for cat in CATEGORY_ORDER:
        items = cards_by_cat.get(cat, [])
        if not items:
            continue
        grid = "\n        ".join(card(p) for p in items)
        groups.append(
            '<div>'
            f'<div class="flex items-baseline justify-between border-b border-edge/60 pb-3">'
            f'<h2 class="text-xl font-extrabold text-white">{cat}</h2>'
            f'<span class="text-xs text-smoke">{len(items)} plugin{"s" if len(items) != 1 else ""}</span>'
            '</div>'
            f'<p class="mt-2 text-sm text-smoke">{CATEGORY_BLURB.get(cat, "")}</p>'
            '<div class="mt-5 grid gap-5 sm:grid-cols-2 lg:grid-cols-3">'
            f'{grid}'
            '</div></div>'
        )
    groups_html = "\n\n      ".join(groups)

    desc = ("loadr's runtime plugin model: install MongoDB, PostgreSQL, MySQL, Redis, Kafka, RabbitMQ and "
            "Elasticsearch protocol drivers on demand. Each clicks through to its own page with install steps, "
            "config and a recorded demo.")
    body = head("Plugins — install only the protocols you need | loadr", desc, "https://loadr.io/plugins/")
    body += f"""
<!-- ======================================================= HERO -->
<section class="hero-grid relative overflow-hidden pt-32 pb-12">
  <div class="pointer-events-none absolute -top-40 left-1/2 h-[44rem] w-[64rem] -translate-x-1/2 rounded-[50%] bg-blood/20 blur-[150px]"></div>
  <div class="mx-auto max-w-5xl px-5 text-center">
    <p class="kicker">Plugins</p>
    <h1 class="mt-3 text-4xl font-extrabold tracking-tight text-white sm:text-5xl">Install only what you need</h1>
    <p class="mx-auto mt-5 max-w-2xl text-lg text-smoke">
      loadr ships a small core. Heavy protocol drivers are delivered as <strong class="text-ash">runtime plugins</strong> —
      they load at runtime, on demand, and are <em>never</em> baked into the binary. One
      <code class="text-flare">loadr plugin install &lt;name&gt;</code> pulls a driver from the signed index, then a
      request to its URL scheme just works.
    </p>
    <div class="mt-6 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 text-sm text-smoke">
      <span class="inline-flex items-center gap-1.5"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2.2"><path d="M20 6 9 17l-5-5"/></svg> Resolved from a signed index</span>
      <span class="inline-flex items-center gap-1.5"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2.2"><path d="M20 6 9 17l-5-5"/></svg> SHA-256 verified</span>
      <span class="inline-flex items-center gap-1.5"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="#4ade80" stroke-width="2.2"><path d="M20 6 9 17l-5-5"/></svg> ABI-checked at load</span>
    </div>
  </div>
</section>

<!-- ======================================================= PLUGIN CARDS -->
<section class="pb-8">
  <div class="mx-auto max-w-6xl space-y-14 px-5">
      {groups_html}
  </div>
</section>

<!-- ======================================================= HOW INSTALL WORKS -->
<section class="border-t border-edge/60 bg-coal/50 py-20">
  <div class="mx-auto max-w-5xl px-5">
    <p class="kicker">How install works</p>
    <h2 class="mt-3 text-3xl font-extrabold tracking-tight text-white">Resolve, verify, load</h2>
    <p class="mt-4 max-w-3xl text-smoke">
      <code class="text-flare">loadr plugin install &lt;name&gt;</code> resolves the name in a signed plugin index — a versioned
      JSON catalogue that maps a short name to the right per-platform artifact. loadr picks the artifact for your host
      target, checks its declared ABI against the one your build provides, downloads it, verifies its
      <strong class="text-ash">SHA-256</strong> against the index, unpacks it and installs it under your plugins
      directory. The native loader re-checks the <code class="text-flare">abi_stable</code> layout at load time as a
      second line of defence.
    </p>
    <div class="mt-8 grid gap-5 lg:grid-cols-2">
      <div class="codebox">
        <div class="codebar"><span>Find &amp; install from the index</span></div>
        <pre><code data-lang="bash"># search the catalogue, then install for this host
$ loadr plugin search mongo
$ loadr plugin install mongo
resolved mongo 1.1.0 (x86_64-unknown-linux-gnu)
sha256 verified ✓   abi 1.0 ✓
installed → ~/.loadr/plugins/mongo</code></pre>
      </div>
      <div class="codebox">
        <div class="codebar"><span>Pin, update, inspect</span></div>
        <pre><code data-lang="bash"># pin a version or override the host target
$ loadr plugin install postgres --version 1.1.0
# re-install newer, ABI-compatible builds
$ loadr plugin update
# list / inspect what's installed
$ loadr plugin list
$ loadr plugin info mysql</code></pre>
      </div>
    </div>
    <p class="mt-6 text-sm text-smoke">
      Full reference: <a class="font-semibold text-flare hover:underline" href="/docs/plugins/installing.html">Installing plugins</a> ·
      <a class="font-semibold text-flare hover:underline" href="/docs/plugins/overview.html">Plugin overview</a>
    </p>
  </div>
</section>

<!-- ======================================================= WRITE YOUR OWN -->
<section class="py-20">
  <div class="mx-auto max-w-5xl px-5">
    <div class="rounded-2xl border border-edge bg-panel p-8 text-center">
      <h2 class="text-2xl font-extrabold text-white">Write your own plugin</h2>
      <p class="mx-auto mt-2 max-w-2xl text-sm text-smoke">
        The same model is open to you. Ship a new protocol, output or helper as a native (<code class="text-flare">abi_stable</code>)
        or WASM plugin, declare its URL schemes, publish it to an index — and anyone can
        <code class="text-flare">loadr plugin install</code> it.
      </p>
      <div class="mt-6 flex flex-wrap justify-center gap-4">
        <a href="/docs/plugins/developing.html" class="glow rounded-xl bg-blood px-6 py-3 font-bold text-white">Write your own →</a>
        <a href="/docs/plugins/publishing.html" class="rounded-xl border border-edge-bright bg-coal px-6 py-3 font-semibold text-ash hover:border-ember/60 hover:text-white">Publish to an index</a>
      </div>
    </div>
  </div>
</section>
"""
    body += FOOTER
    return body


def render_detail(p, prev, nxt):
    slug = p["slug"]
    title = f'{p["name"]} plugin — loadr plugin install {slug}'
    desc = f'{p["tagline"]} Install with `loadr plugin install {slug}` — a runtime driver, sha256-verified and ABI-checked, never baked into the loadr binary.'
    canonical = f"https://loadr.io/plugins/{slug}/"

    parts = [head(title, desc, canonical)]

    # Hero
    parts.append(f"""
<!-- ======================================================= HERO -->
<section class="hero-grid relative overflow-hidden pt-32 pb-10">
  <div class="pointer-events-none absolute -top-40 left-1/2 h-[40rem] w-[56rem] -translate-x-1/2 rounded-[50%] bg-blood/20 blur-[150px]"></div>
  <div class="mx-auto max-w-5xl px-5">
    <nav class="flex items-center gap-2 text-sm text-smoke">
      <a class="hover:text-flare" href="/plugins/">Plugins</a>
      <span class="text-edge-bright">/</span>
      <span class="text-ash">{p["name"]}</span>
    </nav>
    <div class="mt-4 flex items-center gap-4">
      {monogram(p, "h-12 w-12 text-2xl")}
      <h1 class="text-4xl font-black tracking-tight text-white sm:text-5xl">{p["name"]}</h1>
      <span class="rounded-full border border-edge bg-coal px-3 py-1 text-xs text-ash">{p["category"]}</span>
    </div>
    <div class="mt-4">{scheme_badges(p["schemes"])}</div>
    <p class="mt-5 max-w-2xl text-lg text-smoke">{p["description"]}</p>
  </div>
</section>

<!-- ======================================================= DETAIL -->
<section class="pb-16">
  <div class="mx-auto grid max-w-5xl gap-8 px-5 lg:grid-cols-2">
    <div>
      <div class="codebox">
        <div class="codebar"><span>Install</span><button data-copy="#{slug}Install" class="rounded border border-edge px-2 py-0.5 text-[10px] text-smoke hover:text-flare">copy</button></div>
        <pre id="{slug}Install"><code data-lang="bash">$ loadr plugin install {slug}</code></pre>
      </div>
      <div class="codebox mt-4">
        <div class="codebar"><span>{p["yaml_file"]}</span><button data-copy="#{slug}Yaml" class="rounded border border-edge px-2 py-0.5 text-[10px] text-smoke hover:text-flare">copy</button></div>
        <pre id="{slug}Yaml"><code data-lang="yaml">""")
    parts.append(p["yaml"])
    parts.append(f"""</code></pre>
      </div>
      <p class="mt-5 text-sm text-smoke">
        Metrics: {metrics_line(p["metrics"])}.
      </p>
      <div class="mt-6 flex flex-wrap gap-4">
        <a href="{p["docs"]}" class="glow rounded-xl bg-blood px-6 py-3 font-bold text-white">Read the docs →</a>
        <a href="/demos/#{slug}-plugin" class="rounded-xl border border-edge-bright bg-coal px-6 py-3 font-semibold text-ash hover:border-ember/60 hover:text-white">Watch the demo</a>
      </div>
    </div>
    <div>
      <div class="overflow-hidden rounded-xl border border-edge bg-ink">
        <video controls preload="none" aria-label="Demo: installing and running the {p["name"]} plugin" poster="/videos/{p["video"]}-poster.jpg" class="aspect-video w-full bg-ink">
          <source src="/videos/{p["video"]}.mp4" type="video/mp4">
        </video>
      </div>
      <p class="mt-3 text-sm text-smoke">{p["demo_blurb"]} — see <a class="text-flare hover:underline" href="/demos/#{slug}-plugin">demo #{p["demo_num"]}</a>.</p>
      <div class="mt-6 rounded-2xl border border-edge bg-panel p-5 text-sm text-smoke">
        <p class="font-semibold text-white">A runtime plugin, never in the binary</p>
        <p class="mt-2">Installing pulls a per-platform driver from the signed index, verifies its SHA-256 and checks its ABI before it ever loads. Remove it any time with <code class="text-flare">loadr plugin remove {slug}</code>.</p>
      </div>
    </div>
  </div>
</section>

<!-- ======================================================= PREV / NEXT -->
<section class="border-t border-edge/60 bg-coal/40 py-10">
  <div class="mx-auto flex max-w-5xl items-center justify-between gap-4 px-5 text-sm">
""")
    if prev:
        parts.append(f'    <a class="text-smoke hover:text-flare" href="/plugins/{prev["slug"]}/">← {prev["name"]}</a>\n')
    else:
        parts.append('    <span></span>\n')
    parts.append('    <a class="font-semibold text-flare hover:underline" href="/plugins/">All plugins</a>\n')
    if nxt:
        parts.append(f'    <a class="text-smoke hover:text-flare" href="/plugins/{nxt["slug"]}/">{nxt["name"]} →</a>\n')
    else:
        parts.append('    <span></span>\n')
    parts.append("""  </div>
</section>
""")
    parts.append(FOOTER)
    return "".join(parts)


def main():
    out_dir = pathlib.Path(__file__).resolve().parent / "plugins"
    out_dir.mkdir(parents=True, exist_ok=True)

    (out_dir / "index.html").write_text(render_index(), encoding="utf-8")
    print(f"wrote {out_dir / 'index.html'}")

    for i, p in enumerate(PLUGINS):
        prev = PLUGINS[i - 1] if i > 0 else None
        nxt = PLUGINS[i + 1] if i < len(PLUGINS) - 1 else None
        pdir = out_dir / p["slug"]
        pdir.mkdir(parents=True, exist_ok=True)
        (pdir / "index.html").write_text(render_detail(p, prev, nxt), encoding="utf-8")
        print(f"wrote {pdir / 'index.html'}")

    print(f"done: index + {len(PLUGINS)} detail pages")


if __name__ == "__main__":
    main()
