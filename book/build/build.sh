#!/usr/bin/env bash
# Build the designed, loadr-branded PDF of "Performance Testing in Practice".
#
# Pipeline:
#   1. asciidoctor renders the AsciiDoc manuscript (atlas.json order) -> raw HTML
#   2. build_book.py injects the loadr book design (cover, part dividers, ember
#      chapter openers, dark terminal code blocks, callouts, takeaways) and
#      rewrites chapter cross-refs to "Chapter N"
#   3. render_book.js drives headless Chrome (via Playwright) to print a
#      full-bleed cover + a numbered body with running headers
#   4. pdfunite stitches cover + body into the final PDF
#
# Requirements: npx (asciidoctor.js), python3, google-chrome, pdfunite (poppler),
# and Playwright (reused from ../../desktop/node_modules).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOOK="$(cd "$HERE/.." && pwd)"
OUT="${1:-$BOOK/Performance-Testing-in-Practice.pdf}"
TMP="$(mktemp -d)"

echo "==> 1/4 assembling master + rendering HTML"
python3 - "$BOOK" "$TMP" <<'PY'
import json, os, sys
book, tmp = sys.argv[1], sys.argv[2]
m = json.load(open(os.path.join(book, "atlas.json")))
files = [f for f in m["files"] if os.path.basename(f) != "PROPOSAL.adoc"]
with open(os.path.join(tmp, "master.adoc"), "w") as o:
    o.write(f"= {m['title']}\n{m['subtitle']}\n:author: {m['author']}\n"
            ":doctype: book\n:toc:\n:toclevels: 2\n:sectnums:\n:sectnumlevels: 1\n:icons: font\n\n")
    for f in files:
        o.write(f"include::{os.path.join(book, f)}[]\n\n")
PY
npx -y asciidoctor -b html5 -a stylesheet! -a toc=auto -a toclevels=2 \
  -o "$TMP/book_raw.html" "$TMP/master.adoc"

echo "==> 2/4 injecting loadr book design"
BOOK_RAW="$TMP/book_raw.html" COVER_OUT="$TMP/cover.html" BODY_OUT="$TMP/book_styled.html" \
  python3 "$HERE/build_book.py"

echo "==> 3/4 rendering PDF pages (headless Chrome)"
COVER_HTML="$TMP/cover.html" BODY_HTML="$TMP/book_styled.html" \
  COVER_PDF="$TMP/cover.pdf" BODY_PDF="$TMP/body.pdf" \
  node "$HERE/render_book.js"

echo "==> 4/4 stitching final PDF"
pdfunite "$TMP/cover.pdf" "$TMP/body.pdf" "$OUT"
rm -rf "$TMP"
echo "==> done: $OUT"
