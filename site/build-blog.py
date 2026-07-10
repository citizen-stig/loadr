#!/usr/bin/env python3
"""Generate the /blog index + a detail page per post from the POSTS catalog.

Single source of truth for the blog. Writes:
  site/blog/index.html            — categorised card index (Retrospective / Release / Roadmap)
  site/blog/<slug>/index.html     — full article page per post

Each post's prose body is an HTML fragment at site/blog/posts/<slug>.html
(semantic HTML: h2/h3/p/ul/pre/blockquote/table) which the generator wraps in
the branded page chrome and styles consistently. Bodies are READ at build time
so the page can't drift from the committed content.

Adding a post = append one dict to POSTS below, drop its body at
site/blog/posts/<slug>.html, and re-run:
    python3 site/build-blog.py

Output is committed (so Tailwind's content scan sees the classes) and copied to
dist/blog/ by deploy.sh. The shared nav is injected later via the
<!-- INCLUDE:NAV --> marker by build-nav.py.
"""
import html
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
POSTS_DIR = SITE / "blog" / "posts"

# ---------------------------------------------------------------------------
# Data — one record per post. Keys:
#   slug      URL slug under /blog/<slug>/
#   title     card + hero heading
#   category  Retrospective | Release | Roadmap
#   date      ISO date (display + sort)
#   time      24h HH:MM publish time, Europe/London — editorial, author-set.
#             Two posts share a date, so this is what actually orders them.
#   author    byline
#   tag       short kicker label
#   summary   one/two-line card + hero standfirst
#   reading   estimated read time (minutes)
POSTS = [
    {
        "slug": "whats-new",
        "title": "What's New in loadr",
        "category": "Release",
        "date": "2026-07-06",
        "time": "14:05",
        "author": "The loadr team",
        "tag": "Changelog",
        "summary": "Everything shipped so far, release by release — from the first "
                   "protocols to the payload generator and the algorithmic-DoS finder.",
        "reading": 6,
        "pinned": True,
    },
    {
        "slug": "four-weeks",
        "title": "Zero to a k6-and-JMeter competitor in four weeks",
        "category": "Retrospective",
        "date": "2026-07-07",
        "time": "10:20",
        "author": "The loadr team",
        "tag": "Retrospective",
        "summary": "The first commit landed on 12 June. Four weeks and 180 commits "
                   "later, loadr had seven protocols, a plugin ecosystem, a desktop "
                   "app, and a DoS finder. Here's how.",
        "reading": 8,
    },
    {
        "slug": "desktop-in-two-days",
        "title": "The desktop app: seven milestones in forty-eight hours",
        "category": "Retrospective",
        "date": "2026-07-05",
        "time": "16:40",
        "author": "The loadr team",
        "tag": "Retrospective",
        "summary": "M1 to M7 — Electron scaffold to a signed, multi-platform, "
                   "Playwright-tested release — in two days. What we built and what "
                   "we'd do again.",
        "reading": 7,
    },
    {
        "slug": "plugins-not-monolith",
        "title": "A plugin ecosystem, not a monolith",
        "category": "Retrospective",
        "date": "2026-07-03",
        "time": "09:15",
        "author": "The loadr team",
        "tag": "Architecture",
        "summary": "WASM, native Rust, and a plain C ABI so any language can add a "
                   "protocol. How loadr keeps a single binary small while covering "
                   "Postgres, Kafka, MQTT, Cassandra and 25 more.",
        "reading": 7,
    },
    {
        "slug": "finding-dos-bugs",
        "title": "Teaching a load tester to find DoS bugs",
        "category": "Retrospective",
        "date": "2026-07-06",
        "time": "08:30",
        "author": "The loadr team",
        "tag": "Deep dive",
        "summary": "The payload generator and complexity assertion turn \"scale an "
                   "adversarial input and watch response time bend\" into a "
                   "one-command, super-linear-blowup finder.",
        "reading": 6,
    },
    {
        "slug": "six-new-families",
        "title": "What's next: six new feature families",
        "category": "Roadmap",
        "date": "2026-07-08",
        "time": "09:00",
        "author": "The loadr team",
        "tag": "Roadmap",
        "summary": "A complete inbuilt session recorder, spec-driven generation and "
                   "fuzzing, results intelligence, an AI copilot, trace-driven root "
                   "cause, and a resilience suite. What's coming and why.",
        "reading": 9,
    },
]

CAT_ORDER = ["Release", "Retrospective", "Roadmap"]
CAT_BLURB = {
    "Release": "What shipped",
    "Retrospective": "How it was built",
    "Roadmap": "What's coming",
}

# ---------------------------------------------------------------------------
HEAD = """<!doctype html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<script src="/assets/consent.js"></script>
<title>{title}</title>
<meta name="description" content="{desc}">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:url" content="{canonical}">
<meta property="og:type" content="{og_type}">
<link rel="canonical" href="{canonical}">
<link rel="icon" type="image/png" sizes="64x64" href="/assets/favicon-64.png">
<link rel="apple-touch-icon" href="/assets/apple-touch-icon.png">
<link rel="alternate icon" href="/assets/favicon.ico">
<link rel="stylesheet" href="/assets/site.css">
<style>
/* Headings track the rest of the site: sans and tight, not monospace. Mono
   stays where the site already uses it — code, kickers, metadata. */
.prose{{font-size:1.0625rem}}
.prose h2{{font-weight:800;font-size:1.6rem;color:#fff;margin:2.4rem 0 .8rem;letter-spacing:-.015em}}
.prose h3{{font-weight:700;font-size:1.2rem;color:#f3f4f6;margin:1.7rem 0 .5rem;
  padding-left:.6rem;border-left:2px solid #ef4444;letter-spacing:-.01em}}
.prose p{{color:#cbd0d8;line-height:1.75;margin:0 0 1.1rem}}
.prose a{{color:#f87171;text-decoration:none;border-bottom:1px solid rgba(248,113,113,.35)}}
.prose a:hover{{color:#fca5a5}}
.prose strong{{color:#fff;font-weight:700}}
.prose em{{color:#e5e7eb}}
.prose ul,.prose ol{{color:#cbd0d8;line-height:1.7;margin:0 0 1.1rem 1.2rem}}
.prose li{{margin:.3rem 0}}
.prose li::marker{{color:#ef4444}}
.prose code{{font-family:var(--font-mono,ui-monospace,monospace);font-size:.85em;
  background:#141419;color:#f87171;padding:.1rem .35rem;border-radius:.3rem;border:1px solid #232330}}
.prose pre{{background:#0a0a0e;border:1px solid #232330;border-left:3px solid #ef4444;border-radius:.6rem;
  padding:1rem 1.1rem;overflow-x:auto;margin:0 0 1.3rem;font-size:.82rem;line-height:1.6}}
.prose pre code{{background:none;border:0;color:#e5e7eb;padding:0}}
.prose blockquote{{border-left:3px solid #ef4444;margin:0 0 1.3rem;padding:.2rem 0 .2rem 1.1rem;
  color:#e5e7eb;font-style:italic}}
.prose table{{width:100%;border-collapse:collapse;margin:0 0 1.4rem;font-size:.9rem}}
.prose th{{background:#141419;color:#fff;text-align:left;padding:.5rem .7rem;font-family:var(--font-mono,monospace);
  font-size:.78rem;text-transform:uppercase;letter-spacing:.03em}}
.prose td{{padding:.5rem .7rem;border-bottom:1px solid #232330;color:#cbd0d8;vertical-align:top}}
.prose hr{{border:0;border-top:1px solid #232330;margin:2rem 0}}
.prose > *:first-child{{margin-top:0}}
</style>
</head>
<body class="antialiased overflow-x-clip">
<!-- INCLUDE:NAV -->
"""

FOOT = """
<footer class="border-t border-edge/60 bg-coal/40 mt-20">
  <div class="mx-auto max-w-[76rem] px-5 py-10 text-sm text-smoke flex flex-wrap gap-4 justify-between">
    <span>© 2026 loadr — the load testing platform.</span>
    <span><a class="hover:text-flare" href="/blog/">← All posts</a> ·
      <a class="hover:text-flare" href="https://github.com/levantar-ai/loadr">GitHub</a></span>
  </div>
</footer>
<script src="/assets/site.js" defer></script>
</body></html>
"""

CAT_PILL = {
    "Release": "bg-blood/15 text-flare border-blood/30",
    "Retrospective": "bg-panel text-ash border-edge",
    "Roadmap": "bg-ember/10 text-flare border-ember/30",
}


MONTHS = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul",
          "Aug", "Sep", "Oct", "Nov", "Dec"]
TZ = "+01:00"  # Europe/London, BST


def fmt_date(iso):
    y, m, d = iso.split("-")
    return f"{int(d)} {MONTHS[int(m)]} {y}"


def published_iso(p):
    """Machine-readable timestamp for <time datetime=...>."""
    return f"{p['date']}T{p['time']}:00{TZ}"


def sort_key(p):
    """Newest first. Two posts share a date, so the time is what orders them."""
    return (p["date"], p["time"])


def pill_html(p):
    return (f'<span class="rounded-full border px-2.5 py-0.5 text-xs font-semibold '
            f'{CAT_PILL[p["category"]]}">{html.escape(p["tag"])}</span>')


def feed_row(p):
    """One post in the chronological stream: a timestamp rail, then the post."""
    return f"""      <li>
        <a href="/blog/{p['slug']}/" class="group grid gap-x-8 gap-y-3 border-b border-edge/60 py-8 transition-colors hover:bg-coal/30 md:grid-cols-[11rem_minmax(0,1fr)]">
          <div class="md:pl-1">
            <time datetime="{published_iso(p)}" class="block font-mono text-xs text-ash">{fmt_date(p['date'])}</time>
            <span class="mt-1 block font-mono text-xs text-smoke/70">{p['time']} · {p['reading']} min read</span>
          </div>
          <div class="min-w-0 max-w-3xl md:pr-4">
            {pill_html(p)}
            <h3 class="mt-3 text-xl font-bold leading-snug text-white transition-colors group-hover:text-flare">{html.escape(p['title'])}</h3>
            <p class="mt-2 text-[15px] leading-relaxed text-smoke">{html.escape(p['summary'])}</p>
            <span class="mt-3 inline-block text-sm font-semibold text-flare">Read →</span>
          </div>
        </a>
      </li>"""


def build_index():
    # A blog reads as one stream, newest first — not three parallel card grids.
    posts = sorted(POSTS, key=sort_key, reverse=True)
    rows = "\n".join(feed_row(p) for p in posts)

    legend = " · ".join(
        f'<span class="text-ash">{c}</span> <span class="text-smoke/70">{CAT_BLURB[c]}</span>'
        for c in CAT_ORDER
    )

    body = f"""  <main id="main" class="mx-auto max-w-[76rem] px-5 pt-32 pb-20">
    <header class="mb-4 max-w-3xl">
      <p class="font-mono text-xs font-semibold uppercase tracking-[0.18em] text-flare">The loadr blog</p>
      <h1 class="mt-3 text-4xl font-black tracking-tight text-white sm:text-5xl">What we build, and how</h1>
      <p class="mt-4 text-lg leading-relaxed text-smoke">Release notes, build retrospectives, and where loadr is headed next.</p>
      <p class="mt-5 text-xs text-smoke/70">{legend}</p>
    </header>

    <ol class="mt-10 border-t border-edge/60">
{rows}
    </ol>
  </main>"""
    head = HEAD.format(
        title="Blog — loadr",
        desc="Release notes, retrospectives and roadmap for loadr, the load testing platform.",
        canonical="https://loadr.io/blog/",
        og_type="website",
    )
    (SITE / "blog").mkdir(parents=True, exist_ok=True)
    (SITE / "blog" / "index.html").write_text(head + body + FOOT)
    print("wrote blog/index.html")


def meta_row(label, value):
    return (
        '<div>'
        f'<dt class="font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-smoke/60">{label}</dt>'
        f'<dd class="mt-1 text-sm text-ash">{value}</dd>'
        '</div>'
    )


def meta_aside(p):
    """Right-hand column: the post's metadata, plus where to go next."""
    others = [q for q in sorted(POSTS, key=sort_key, reverse=True) if q["slug"] != p["slug"]][:3]
    more = "".join(
        f'<li><a href="/blog/{q["slug"]}/" class="group block py-2">'
        f'<span class="block text-sm leading-snug text-ash group-hover:text-flare">{html.escape(q["title"])}</span>'
        f'<time datetime="{published_iso(q)}" class="mt-0.5 block font-mono text-[11px] text-smoke/60">'
        f'{fmt_date(q["date"])} · {q["time"]}</time></a></li>'
        for q in others
    )
    return f"""    <aside class="mt-12 lg:mt-0 lg:w-[19rem] lg:shrink-0">
      <div class="lg:sticky lg:top-28">
        <div class="rounded-2xl border border-edge bg-panel p-5">
          <dl class="space-y-4">
            {meta_row("Published", f'<time datetime="{published_iso(p)}">{fmt_date(p["date"])} · {p["time"]}</time>')}
            {meta_row("Author", html.escape(p['author']))}
            {meta_row("Reading time", f"{p['reading']} min")}
            {meta_row("Category", f'<a class="hover:text-flare" href="/blog/">{html.escape(p["category"])}</a>')}
          </dl>
        </div>
        <div class="mt-6 rounded-2xl border border-edge bg-panel p-5">
          <p class="font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-smoke/60">More posts</p>
          <ul class="mt-2 divide-y divide-edge/60">{more}</ul>
          <a href="/blog/" class="mt-4 inline-block text-sm font-semibold text-flare hover:underline">All posts →</a>
        </div>
      </div>
    </aside>"""


def build_post(p):
    body_path = POSTS_DIR / f"{p['slug']}.html"
    if not body_path.exists():
        print(f"  ! missing body: posts/{p['slug']}.html — skipping")
        return False
    prose = body_path.read_text()
    head = HEAD.format(
        title=f"{p['title']} — loadr blog",
        desc=html.escape(p["summary"]),
        canonical=f"https://loadr.io/blog/{p['slug']}/",
        og_type="article",
    )
    # The article fills the container; metadata moves out to its own column.
    article = f"""  <main id="main" class="mx-auto max-w-[76rem] px-5 pt-32 pb-20">
    <nav class="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-smoke">
      <a class="hover:text-flare" href="/blog/">Blog</a>
      <span class="text-edge-bright">/</span>
      <span class="text-smoke">{html.escape(p['category'])}</span>
      <span class="text-edge-bright">/</span>
      <span class="text-ash">{html.escape(p['title'])}</span>
    </nav>

    <header class="mt-5 border-b border-edge/60 pb-8">
      <div class="flex flex-wrap items-center gap-3">
        {pill_html(p)}
        <time datetime="{published_iso(p)}" class="font-mono text-xs text-smoke">{fmt_date(p['date'])} · {p['time']}</time>
        <span class="font-mono text-xs text-smoke/70">{p['reading']} min read</span>
      </div>
      <h1 class="mt-4 text-4xl font-black leading-tight tracking-tight text-white sm:text-5xl">{html.escape(p['title'])}</h1>
      <p class="mt-4 max-w-3xl text-lg leading-relaxed text-smoke">{html.escape(p['summary'])}</p>
    </header>

    <div class="lg:flex lg:gap-12 xl:gap-16">
      <article class="prose min-w-0 flex-1 pt-10">
{prose}
      </article>
{meta_aside(p)}
    </div>
  </main>"""
    out = SITE / "blog" / p["slug"]
    out.mkdir(parents=True, exist_ok=True)
    (out / "index.html").write_text(head + article + FOOT)
    print(f"wrote blog/{p['slug']}/index.html")
    return True


if __name__ == "__main__":
    POSTS_DIR.mkdir(parents=True, exist_ok=True)
    build_index()
    n = sum(build_post(p) for p in POSTS)
    print(f"done: index + {n}/{len(POSTS)} posts")
