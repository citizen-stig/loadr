#!/usr/bin/env python3
"""Inject the canonical nav partial into built HTML pages.

Replaces every `<!-- INCLUDE:NAV -->` marker found in the *.html files under
DIST with the contents of the nav partial. This keeps the site nav as a single
source of truth (site/partials/nav.html) instead of hand-copied per page.

Usage: build-nav.py <partial.html> <dist-dir>

Fails loudly (non-zero exit) if the partial is missing or no marker is found,
so a broken build can never silently ship pages with no nav.
"""
import sys
from pathlib import Path

MARKER = "<!-- INCLUDE:NAV -->"


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: build-nav.py <partial.html> <dist-dir>", file=sys.stderr)
        return 2

    partial_path = Path(sys.argv[1])
    dist = Path(sys.argv[2])

    if not partial_path.is_file():
        print(f"error: nav partial not found: {partial_path}", file=sys.stderr)
        return 1

    # Inject the partial verbatim. Its leading HTML comment is a valid,
    # non-rendering comment and ships harmlessly — we deliberately do NOT try to
    # strip it, because cutting at the first "-->" mis-parses any "-->" that
    # appears inside the comment text and leaks the tail as visible page text.
    partial = partial_path.read_text(encoding="utf-8").lstrip("\n")

    total = 0
    pages = 0
    for html in sorted(dist.rglob("*.html")):
        text = html.read_text(encoding="utf-8")
        if MARKER not in text:
            continue
        count = text.count(MARKER)
        html.write_text(text.replace(MARKER, partial), encoding="utf-8")
        total += count
        pages += 1
        print(f"    nav -> {html.relative_to(dist)}")

    if total == 0:
        print(f"error: no '{MARKER}' marker found under {dist}", file=sys.stderr)
        return 1

    print(f"    injected nav into {pages} page(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
