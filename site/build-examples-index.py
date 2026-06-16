#!/usr/bin/env python3
"""Generate a themed, browsable index of the examples/ directory.

Reads every examples/*.yaml, pulls out the leading `# ...` comment block and the
`name:` field, and writes a static HTML page (matching the loadr.io theme) that
links to each raw file plus a download bundle.

Usage: build-examples-index.py <examples_dir> <out_html>
"""
import html
import re
import sys
from pathlib import Path


def describe(path: Path):
    """Return (name, blurb) for one example file."""
    name = path.stem
    blurb_lines = []
    yaml_name = None
    with path.open(encoding="utf-8") as fh:
        in_header = True
        for line in fh:
            stripped = line.rstrip("\n")
            if in_header and stripped.startswith("#"):
                text = stripped.lstrip("#").strip()
                if text:
                    blurb_lines.append(text)
                continue
            in_header = False
            m = re.match(r"\s*name:\s*(.+?)\s*$", stripped)
            if m and yaml_name is None:
                yaml_name = m.group(1).strip().strip("\"'")
    blurb = " ".join(blurb_lines).strip()
    return yaml_name or name, blurb


def main():
    examples_dir = Path(sys.argv[1])
    out_html = Path(sys.argv[2])

    files = sorted(examples_dir.glob("*.yaml"))
    cards = []
    for f in files:
        title, blurb = describe(f)
        fname = html.escape(f.name)
        cards.append(
            '<a class="ex-card" href="/examples/{fn}">'
            '<div class="ex-file">{fn}</div>'
            '<div class="ex-name">{title}</div>'
            '<p class="ex-blurb">{blurb}</p>'
            "</a>".format(
                fn=fname,
                title=html.escape(title),
                blurb=html.escape(blurb) or "Runnable loadr example.",
            )
        )

    count = len(files)
    page = """<!doctype html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<!-- Google Analytics (gtag.js) via Consent Mode; see /assets/consent.js -->
<script src="/assets/consent.js"></script>
<title>Examples — loadr</title>
<meta name="description" content="{count} runnable loadr examples: ramp and spike tests, data-driven logins, WebSocket, gRPC, GraphQL, Redis, SSE, browser, distributed fleets and more.">
<link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/assets/site.css">
<style>
  .ex-grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));gap:14px;margin-top:2.5rem}}
  .ex-card{{display:block;border:1px solid var(--color-edge);background:var(--color-panel);border-radius:.9rem;padding:1.1rem 1.25rem;transition:border-color .15s ease,transform .15s ease}}
  .ex-card:hover{{border-color:#ef444480;transform:translateY(-2px)}}
  .ex-file{{font-family:var(--font-mono);font-size:.72rem;color:var(--color-ember);letter-spacing:.02em}}
  .ex-name{{margin-top:.35rem;font-weight:700;color:#fff}}
  .ex-blurb{{margin-top:.5rem;font-size:.82rem;line-height:1.5;color:var(--color-smoke)}}
</style>
</head>
<body class="antialiased">
<header class="fixed top-0 inset-x-0 z-50 border-b border-edge/70 bg-ink/85 backdrop-blur">
  <nav class="mx-auto flex h-16 max-w-7xl items-center justify-between px-5">
    <a href="/" class="flex items-center gap-2">
      <svg width="22" height="22" viewBox="0 0 32 32"><path d="M18 2 L8 18 L15 18 L13 30 L24 13 L17 13 Z" fill="#ef4444"/></svg>
      <span class="text-lg font-extrabold tracking-tight text-white">loadr<span class="text-ember">.io</span></span>
    </a>
    <div class="hidden items-center gap-7 text-sm font-medium text-smoke lg:flex">
      <a class="hover:text-flare" href="/demos/">Demos</a>
      <a class="hover:text-flare" href="/#features">Features</a>
      <a class="hover:text-flare" href="/#protocols">Protocols</a>
      <a class="hover:text-flare" href="/#distributed">Distributed</a>
      <a class="hover:text-flare" href="/#webui">Web UI</a>
      <a class="hover:text-flare" href="/#compare">Compare</a>
      <a class="hover:text-flare" href="/#credits">Built on</a>
      <a class="hover:text-flare" href="/#roadmap">Roadmap</a>
      <a class="hover:text-flare" href="/plugins/">Plugins</a>
      <a class="hover:text-flare" href="/docs/">Docs</a>
      <a class="hover:text-flare" href="https://github.com/levantar-ai/loadr" rel="noopener">GitHub</a>
      <a href="/download/" class="glow rounded-lg bg-blood px-4 py-2 font-semibold text-white transition">Download</a>
    </div>
    <button id="navToggle" class="rounded-md border border-edge p-2 text-smoke lg:hidden" aria-label="menu">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 7h16M4 12h16M4 17h16"/></svg>
    </button>
  </nav>
  <div id="navMenu" class="hidden border-t border-edge bg-coal px-5 py-4 lg:hidden">
    <div class="grid gap-3 text-sm font-medium text-smoke">
      <a href="/demos/">Demos</a><a href="/#features">Features</a><a href="/#protocols">Protocols</a>
      <a href="/#distributed">Distributed</a><a href="/#webui">Web UI</a><a href="/#compare">Compare</a>
      <a href="/#credits">Built on</a><a href="/#roadmap">Roadmap</a>
      <a href="/plugins/">Plugins</a><a href="/docs/">Docs</a>
      <a href="https://github.com/levantar-ai/loadr" rel="noopener">GitHub</a>
      <a href="/download/">Download</a>
    </div>
  </div>
</header>

<section class="pt-32 pb-24">
  <div class="mx-auto max-w-7xl px-5">
    <p class="kicker">Examples</p>
    <h1 class="mt-3 text-4xl font-black tracking-tight text-white sm:text-5xl">{count} runnable examples</h1>
    <p class="mt-4 max-w-2xl text-smoke">Every one is a complete, valid test you can run with <code class="text-flare">loadr run &lt;file&gt;</code>. They ship in the <code class="text-flare">examples/</code> folder of every download, or grab them here.</p>
    <div class="mt-8 flex flex-wrap gap-4">
      <a href="/examples.tar.gz" class="glow rounded-xl bg-blood px-6 py-3.5 font-bold text-white">Download all (.tar.gz)</a>
      <a href="/download/" class="rounded-xl border border-edge-bright bg-panel px-6 py-3.5 font-semibold text-ash hover:border-ember/60 hover:text-white">Get loadr</a>
    </div>
    <div class="ex-grid">
      {cards}
    </div>
  </div>
</section>

<footer class="border-t border-edge/60 py-10">
  <div class="mx-auto max-w-7xl px-5 text-xs text-smoke">© 2026 loadr. Built in Rust. <a class="hover:text-flare" href="/">← back to loadr.io</a></div>
</footer>
<script src="/assets/site.js" defer></script>
</body>
</html>
""".format(count=count, cards="\n      ".join(cards))

    out_html.parent.mkdir(parents=True, exist_ok=True)
    out_html.write_text(page, encoding="utf-8")
    print("wrote {} ({} examples)".format(out_html, count))


if __name__ == "__main__":
    main()
