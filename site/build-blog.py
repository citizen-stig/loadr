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
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{desc}">
<link rel="icon" type="image/png" sizes="64x64" href="/assets/favicon-64.png">
<link rel="apple-touch-icon" href="/assets/apple-touch-icon.png">
<link rel="alternate icon" href="/assets/favicon.ico">
<link rel="stylesheet" href="/assets/site.css">
<style>
.prose h2{{font-family:var(--font-mono,ui-monospace,monospace);font-weight:800;font-size:1.5rem;
  color:#fff;margin:2.2rem 0 .8rem;letter-spacing:-.01em}}
.prose h3{{font-family:var(--font-mono,ui-monospace,monospace);font-weight:700;font-size:1.15rem;
  color:#f3f4f6;margin:1.6rem 0 .5rem;padding-left:.6rem;border-left:2px solid #ef4444}}
.prose p{{color:#cbd0d8;line-height:1.75;margin:0 0 1rem}}
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
</style>
</head>
<body class="bg-ink text-ash antialiased">
<!-- INCLUDE:NAV -->
"""

FOOT = """
<footer class="border-t border-edge mt-20">
  <div class="mx-auto max-w-3xl px-5 py-10 text-sm text-smoke flex flex-wrap gap-4 justify-between">
    <span>© loadr — the load testing platform.</span>
    <span><a class="hover:text-flare" href="/blog/">← All posts</a> ·
      <a class="hover:text-flare" href="https://github.com/levantar-ai/loadr">GitHub</a></span>
  </div>
</footer>
</body></html>
"""

CAT_PILL = {
    "Release": "bg-blood/15 text-flare border-blood/30",
    "Retrospective": "bg-panel text-ash border-edge",
    "Roadmap": "bg-ember/10 text-flare border-ember/30",
}


def fmt_date(iso):
    y, m, d = iso.split("-")
    months = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul",
              "Aug", "Sep", "Oct", "Nov", "Dec"]
    return f"{int(d)} {months[int(m)]} {y}"


def card(p):
    pill = CAT_PILL[p["category"]]
    return f"""      <a href="/blog/{p['slug']}/" class="group flex flex-col rounded-2xl border border-edge bg-coal p-6 transition hover:border-ember/50 hover:bg-panel">
        <div class="flex items-center gap-3 mb-3">
          <span class="rounded-full border px-2.5 py-0.5 text-xs font-semibold {pill}">{html.escape(p['tag'])}</span>
          <span class="text-xs text-smoke">{fmt_date(p['date'])} · {p['reading']} min</span>
        </div>
        <h3 class="text-lg font-bold text-white group-hover:text-flare leading-snug">{html.escape(p['title'])}</h3>
        <p class="mt-2 text-sm text-smoke leading-relaxed">{html.escape(p['summary'])}</p>
        <span class="mt-4 text-sm font-semibold text-flare">Read →</span>
      </a>"""


def build_index():
    by_cat = {c: [] for c in CAT_ORDER}
    for p in POSTS:
        by_cat[p["category"]].append(p)
    for c in by_cat:
        by_cat[c].sort(key=lambda p: p["date"], reverse=True)

    sections = []
    for c in CAT_ORDER:
        if not by_cat[c]:
            continue
        cards = "\n".join(card(p) for p in by_cat[c])
        sections.append(f"""    <section class="mb-14">
      <div class="flex items-baseline gap-3 mb-5">
        <h2 class="text-xl font-black text-white">{c}</h2>
        <span class="text-sm text-smoke">{CAT_BLURB[c]}</span>
      </div>
      <div class="grid gap-5 sm:grid-cols-2 lg:grid-cols-3">
{cards}
      </div>
    </section>""")

    body = f"""  <main id="main" class="mx-auto max-w-6xl px-5 py-16">
    <header class="mb-12">
      <p class="text-sm font-semibold uppercase tracking-widest text-flare">The loadr blog</p>
      <h1 class="mt-2 text-4xl font-black text-white sm:text-5xl">What we build, and how</h1>
      <p class="mt-4 max-w-2xl text-lg text-smoke">Release notes, build retrospectives, and where loadr is headed next.</p>
    </header>
{''.join(sections)}
  </main>"""
    head = HEAD.format(title="Blog — loadr",
                       desc="Release notes, retrospectives and roadmap for loadr, the load testing platform.")
    (SITE / "blog").mkdir(parents=True, exist_ok=True)
    (SITE / "blog" / "index.html").write_text(head + body + FOOT)
    print("wrote blog/index.html")


def build_post(p):
    body_path = POSTS_DIR / f"{p['slug']}.html"
    if not body_path.exists():
        print(f"  ! missing body: posts/{p['slug']}.html — skipping")
        return False
    prose = body_path.read_text()
    pill = CAT_PILL[p["category"]]
    head = HEAD.format(title=f"{p['title']} — loadr blog", desc=html.escape(p["summary"]))
    article = f"""  <main id="main" class="mx-auto max-w-3xl px-5 py-16">
    <a href="/blog/" class="text-sm text-smoke hover:text-flare">← Blog</a>
    <header class="mt-4 mb-10 border-b border-edge pb-8">
      <div class="flex items-center gap-3 mb-4">
        <span class="rounded-full border px-2.5 py-0.5 text-xs font-semibold {pill}">{html.escape(p['tag'])}</span>
        <span class="text-xs text-smoke">{fmt_date(p['date'])} · {p['reading']} min read · {html.escape(p['author'])}</span>
      </div>
      <h1 class="text-4xl font-black text-white leading-tight">{html.escape(p['title'])}</h1>
      <p class="mt-4 text-lg text-smoke leading-relaxed">{html.escape(p['summary'])}</p>
    </header>
    <article class="prose">
{prose}
    </article>
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
