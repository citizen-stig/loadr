# Building the designed PDF

Renders the AsciiDoc manuscript into a **loadr-branded** print PDF —
dark ember cover, part dividers, big ember chapter openers, dark terminal-style
code blocks, colored callouts, on-brand "Key Takeaways" boxes, and running
headers with ember page numbers (7×9.25″ trim, ~275pp).

## Run

```bash
bash book/build/build.sh                       # -> book/Performance-Testing-in-Practice.pdf
bash book/build/build.sh /path/to/out.pdf      # custom output path
```

## Pipeline

1. **assemble + render** — `atlas.json` file order → a master `.adoc` → HTML via
   `asciidoctor` (`npx asciidoctor`).
2. **design** — `build_book.py` injects the print stylesheet, a branded cover,
   part dividers, chapter openers, and rewrites chapter cross-refs to
   "Chapter N".
3. **print** — `render_book.js` drives headless Chrome (via Playwright) to print
   a full-bleed cover and a numbered body with running headers.
4. **stitch** — `pdfunite` joins cover + body.

## Requirements

- `npx` (pulls `asciidoctor.js` on first run)
- `python3`
- `google-chrome` (`CHROME_PATH` to override)
- `pdfunite` (poppler-utils)
- Playwright — reused from `../../desktop/node_modules` (`PLAYWRIGHT_PATH` to override)

The output PDF is **regenerable** and intentionally not committed; run `build.sh`
to produce it. Brand tokens (ink/ember palette, JetBrains Mono) are pulled from
the loadr site and live in `build_book.py`.
