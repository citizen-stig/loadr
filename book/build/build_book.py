#!/usr/bin/env python3
import re, base64, html, os

# Repo root defaults to two levels up from this script (book/build/ -> repo).
ROOT = os.environ.get("REPO_ROOT",
        os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))
RAW  = os.environ.get("BOOK_RAW", "/tmp/book_raw.html")
COVER_OUT = os.environ.get("COVER_OUT", "/tmp/cover.html")
BODY_OUT  = os.environ.get("BODY_OUT",  "/tmp/book_styled.html")
logo_b64 = base64.b64encode(open(f"{ROOT}/site/assets/logo-mark.png","rb").read()).decode()
LOGO = f"data:image/png;base64,{logo_b64}"

# ---------------------------------------------------------------- CSS
CSS = r"""
:root{
  --ink:#07070a; --coal:#0d0d12; --panel:#141419; --edge:#232330;
  --blood:#dc2626; --ember:#ef4444; --flare:#f87171;
  --smoke:#6b7280; --ash:#d1d5db; --body:#17171c;
  --mono:"JetBrainsMono Nerd Font","DejaVu Sans Mono",monospace;
  --serif:"DejaVu Serif",Georgia,"Times New Roman",serif;
}
@page{ size:7in 9.25in; margin:0.85in 0.78in 0.72in; }
*{ box-sizing:border-box; }
html{ font-size:10.3pt; -webkit-print-color-adjust:exact; print-color-adjust:exact; }
body{ font-family:var(--serif); color:var(--body); line-height:1.5; margin:0; }
p{ margin:0 0 0.62em; text-align:justify; hyphens:auto; }
a{ color:var(--blood); text-decoration:none; }
strong{ color:#000; }
em{ }

/* ---- hide asciidoctor's built-in title block (cover replaces it) ---- */
#header > h1, #header .details{ display:none; }
#header{ margin:0; padding:0; }

/* ---- table of contents ---- */
#toc{ page-break-after:always; padding-top:0.15in; }
#toctitle{ font-family:var(--mono); font-weight:800; font-size:9pt; letter-spacing:6px;
  text-transform:uppercase; color:var(--ember); margin:0 0 4px; }
#toctitle + *, #toc::after{}
#toc > ul::before{ content:"Contents"; display:block; font-family:var(--mono); font-weight:800;
  font-size:23pt; color:var(--ink); letter-spacing:-0.5px; margin:0 0 0.28in; }
#toc ul{ list-style:none; margin:0; padding:0; }
#toc .sectlevel1 > li{ break-inside:avoid; }
#toc .sectlevel1 > li > a{ display:flex; align-items:baseline; font-family:var(--mono);
  font-weight:700; font-size:10pt; color:var(--ink); padding:8px 0 4px; margin-top:6px;
  border-bottom:1px solid #ededf1; }
#toc .sectlevel1 > li > a::before{ content:"▪"; color:var(--ember); font-size:7pt; margin-right:11px;
  position:relative; top:-1px; }
#toc .sectlevel2{ padding:2px 0 6px 24px; }
#toc .sectlevel2 a{ font-family:var(--serif); font-size:8.4pt; color:var(--smoke); line-height:1.5; }
#toc .sectlevel2 li{ padding:1px 0; }

/* ---- part divider (full page) ---- */
.partdiv{ page-break-before:always; page-break-after:always; height:7.6in; display:flex;
  flex-direction:column; justify-content:center; }
.partdiv .pd-num{ font-family:var(--mono); font-weight:800; font-size:13pt; letter-spacing:8px;
  text-transform:uppercase; color:var(--ember); }
.partdiv .pd-rule{ width:2.2in; height:3px; background:var(--ink); margin:0.18in 0 0.28in; }
.partdiv .pd-title{ font-family:var(--mono); font-weight:800; font-size:34pt; line-height:1.05;
  letter-spacing:-1px; color:var(--ink); margin:0; }
.partdiv .pd-sub{ font-family:var(--mono); font-size:9pt; letter-spacing:3px; text-transform:uppercase;
  color:var(--smoke); margin-top:0.3in; }

/* ---- chapter / section openers ---- */
.sect1{ page-break-before:always; }
.chapopen{ display:flex; align-items:flex-end; gap:16px; border-bottom:2.5px solid var(--ember);
  padding-bottom:9px; margin:0.25in 0 0; }
.chapopen .co-num{ font-family:var(--mono); font-weight:800; font-size:58pt; line-height:0.8; color:var(--ember); }
.chapopen .co-word{ font-family:var(--mono); font-weight:700; font-size:8.5pt; letter-spacing:5px;
  text-transform:uppercase; color:var(--smoke); padding-bottom:12px; }
.sect1 > h2{ font-family:var(--mono); font-weight:800; font-size:22pt; line-height:1.12;
  letter-spacing:-0.6px; color:var(--ink); margin:14px 0 0.3in; border:none; }
.sect1.frontish > h2{ margin-top:0; }

/* ---- sub-headings ---- */
.sect2{ break-inside:auto; }
.sect2 > h3{ font-family:var(--mono); font-weight:700; font-size:12.5pt; color:var(--ink);
  margin:1.5em 0 0.35em; padding-left:12px; border-left:3px solid var(--ember); line-height:1.2; }
.sect3 > h4{ font-family:var(--mono); font-weight:700; font-size:10pt; color:#2b2b32;
  margin:1.2em 0 0.25em; }
h2,h3,h4{ page-break-after:avoid; }

/* ---- body lists ---- */
ul,ol{ margin:0 0 0.65em; padding-left:1.25em; }
li{ margin:0.18em 0; }
.dlist dt{ font-weight:700; font-family:var(--mono); font-size:9.2pt; margin-top:0.5em; }
.dlist dd{ margin:0 0 0.4em 1.2em; }

/* ---- inline + block code ---- */
code{ font-family:var(--mono); font-size:0.84em; background:#f1f1f4; color:#b91c1c;
  padding:0.5px 4px; border-radius:3px; }
.listingblock{ margin:0.85em 0; break-inside:avoid; }
.listingblock pre.highlight{ position:relative; background:var(--coal); color:#e5e7eb;
  border:1px solid var(--edge); border-left:3px solid var(--ember); border-radius:7px;
  padding:19px 15px 13px; font-family:var(--mono); font-size:8.1pt; line-height:1.5;
  white-space:pre-wrap; word-break:break-word; overflow:hidden; }
.listingblock pre.highlight code{ background:none; color:inherit; padding:0; font-size:inherit; border-radius:0; }
.listingblock code[data-lang]::before{ content:attr(data-lang); position:absolute; top:6px; right:13px;
  font-size:6.6pt; letter-spacing:1.5px; text-transform:uppercase; color:var(--flare); font-weight:700; opacity:0.85; }
.literalblock pre{ background:var(--coal); color:#e5e7eb; border-radius:7px; padding:12px 15px;
  font-family:var(--mono); font-size:8.1pt; white-space:pre-wrap; break-inside:avoid; }

/* ---- admonitions ---- */
.admonitionblock{ margin:1em 0; break-inside:avoid; }
.admonitionblock > table{ border-collapse:collapse; width:100%; }
.admonitionblock td.icon{ display:none; }
.admonitionblock td.content{ padding:11px 15px; border-left:3px solid var(--ember);
  background:#fbfbfc; border-radius:0 6px 6px 0; }
.admonitionblock td.content > :first-child{ margin-top:0; }
.admonitionblock td.content::before{ display:block; font-family:var(--mono); font-weight:700;
  font-size:7.6pt; letter-spacing:2.5px; text-transform:uppercase; margin-bottom:7px; }
.admonitionblock.note td.content{ border-left-color:#3b82f6; background:#f6f9ff; }
.admonitionblock.note td.content::before{ content:"Note"; color:#3b82f6; }
.admonitionblock.tip td.content{ border-left-color:#16a34a; background:#f5fdf8; }
.admonitionblock.tip td.content::before{ content:"Tip"; color:#16a34a; }
.admonitionblock.warning td.content{ border-left-color:var(--blood); background:#fff6f6; }
.admonitionblock.warning td.content::before{ content:"Warning"; color:var(--blood); }
.admonitionblock.important td.content{ border-left-color:var(--ember); background:#fff7f5; }
.admonitionblock.important td.content::before{ content:"Important"; color:var(--ember); }
.admonitionblock.caution td.content{ border-left-color:#d97706; background:#fffbf3; }
.admonitionblock.caution td.content::before{ content:"Caution"; color:#d97706; }

/* ---- key-takeaways sidebar (dark, on-brand) ---- */
.sidebarblock{ margin:1.3em 0 0.6em; break-inside:avoid; background:var(--ink);
  border-radius:9px; padding:2px; }
.sidebarblock > .content{ padding:17px 20px; }
.sidebarblock .title{ font-family:var(--mono); font-weight:800; font-size:8.5pt; letter-spacing:3px;
  text-transform:uppercase; color:var(--ember); margin:0 0 11px; }
.sidebarblock .title::before{ content:"◆ "; color:var(--ember); }
.sidebarblock p, .sidebarblock li{ color:var(--ash); font-size:9.3pt; line-height:1.5; }
.sidebarblock ul{ margin:0; padding-left:15px; }
.sidebarblock li{ margin:6px 0; }
.sidebarblock strong{ color:#fff; }
.sidebarblock code{ background:#22222b; color:var(--flare); }

/* ---- field card: the recurring, tangible cheat-sheet (light, distinct from takeaways) ---- */
.sidebarblock.fieldcard{ background:#fbfaf7; border:1px solid var(--edge); border-top:3px solid var(--ember);
  border-radius:9px; padding:0; }
.sidebarblock.fieldcard > .content{ padding:15px 18px; }
.sidebarblock.fieldcard .title{ color:var(--ember); font-family:var(--mono); font-weight:800; font-size:8.5pt;
  letter-spacing:3px; text-transform:uppercase; margin:0 0 10px; }
.sidebarblock.fieldcard .title::before{ content:"\26A1  "; }
.sidebarblock.fieldcard p, .sidebarblock.fieldcard li{ color:var(--body); font-size:8.8pt; line-height:1.45; }
.sidebarblock.fieldcard strong{ color:#000; }
.sidebarblock.fieldcard code{ background:#efe7e7; color:#b91c1c; }
.sidebarblock.fieldcard .dlist dt{ color:var(--ink); font-family:var(--mono); font-weight:700;
  font-size:8.3pt; margin-top:7px; }
.sidebarblock.fieldcard .dlist dt::before{ content:"\25B8  "; color:var(--ember); }
.sidebarblock.fieldcard .dlist dd{ margin:1px 0 0 1.15em; font-size:8.6pt; color:#33333a; }
.sidebarblock.fieldcard .listingblock pre.highlight{ font-size:7.6pt; padding:12px 13px 10px; margin:9px 0; }
.sidebarblock.fieldcard ul{ padding-left:14px; } .sidebarblock.fieldcard li{ margin:3px 0; }

/* ---- content tables ---- */
table.tableblock{ border-collapse:collapse; width:100%; margin:1.05em 0; font-size:8.8pt; break-inside:avoid; }
table.tableblock caption{ caption-side:bottom; font-size:7.6pt; color:var(--smoke); font-family:var(--mono);
  text-align:left; padding-top:5px; }
.tableblock th{ background:var(--ink); color:#fff; font-family:var(--mono); font-weight:700;
  font-size:7.4pt; letter-spacing:0.6px; text-transform:uppercase; padding:8px 10px; text-align:left; }
.tableblock td{ padding:6px 10px; border-bottom:1px solid #e7e7ec; vertical-align:top; line-height:1.4; }
table.tableblock tr:nth-child(even) td{ background:#fafafb; }

/* ---- quotes ---- */
.quoteblock{ margin:1.1em 0; break-inside:avoid; }
.quoteblock blockquote{ border-left:3px solid var(--ember); margin:0; padding:2px 0 2px 18px;
  font-style:italic; font-size:11pt; color:#2c2c33; }
.quoteblock .attribution{ font-style:normal; font-family:var(--mono); font-size:8pt;
  color:var(--smoke); margin-top:6px; }

/* ---- misc ---- */
.imageblock, .image{ text-align:center; }
hr{ border:none; border-top:1px solid var(--edge); margin:1.4em 0; }
.title{ font-style:italic; color:var(--smoke); font-size:8.6pt; }
"""

# ---------------------------------------------------------------- cover
COVER = f"""<!doctype html><html><head><meta charset="utf-8"><style>
*{{margin:0;padding:0;box-sizing:border-box;-webkit-print-color-adjust:exact;print-color-adjust:exact;}}
html,body{{width:7in;height:9.25in;}}
.cover{{position:relative;width:7in;height:9.25in;background:#07070a;overflow:hidden;
  font-family:"JetBrainsMono Nerd Font","DejaVu Sans Mono",monospace;color:#fff;}}
.glow{{position:absolute;top:-2.2in;right:-2in;width:6in;height:6in;border-radius:50%;
  background:radial-gradient(circle,rgba(239,68,68,0.42) 0%,rgba(239,68,68,0.10) 40%,transparent 70%);}}
.glow2{{position:absolute;bottom:-2.5in;left:-2in;width:5.5in;height:5.5in;border-radius:50%;
  background:radial-gradient(circle,rgba(220,38,38,0.20) 0%,transparent 68%);}}
.grid{{position:absolute;inset:0;opacity:0.05;
  background-image:linear-gradient(#fff 1px,transparent 1px),linear-gradient(90deg,#fff 1px,transparent 1px);
  background-size:0.5in 0.5in;}}
.inner{{position:relative;height:100%;padding:0.95in 0.85in;display:flex;flex-direction:column;}}
.brandrow{{display:flex;align-items:center;gap:12px;}}
.brandrow img{{width:42px;height:42px;}}
.brandrow .bn{{font-weight:800;font-size:20pt;letter-spacing:-0.5px;}}
.brandrow .bn .dot{{color:#ef4444;}}
.kicker{{margin-top:1.9in;font-size:9pt;letter-spacing:6px;text-transform:uppercase;color:#ef4444;font-weight:700;}}
.title{{margin-top:0.22in;font-size:41pt;font-weight:800;line-height:1.02;letter-spacing:-1.5px;color:#fff;}}
.title .em{{color:#ef4444;}}
.rule{{width:2.4in;height:3px;background:#ef4444;margin:0.35in 0 0.28in;}}
.sub{{font-family:"DejaVu Serif",Georgia,serif;font-style:italic;font-size:14.5pt;color:#d1d5db;line-height:1.4;max-width:4.6in;}}
.spacer{{flex:1;}}
.termcard{{background:rgba(13,13,18,0.85);border:1px solid #232330;border-left:3px solid #ef4444;
  border-radius:8px;padding:12px 15px;font-size:8.6pt;color:#e5e7eb;line-height:1.7;max-width:4.4in;}}
.termcard .p{{color:#6b7280;}} .termcard .c{{color:#ef4444;}} .termcard .g{{color:#4ade80;}} .termcard .o{{color:#fb923c;}}
.author{{margin-top:0.5in;font-size:11pt;font-weight:700;letter-spacing:1px;color:#fff;}}
.pubrow{{margin-top:6px;font-size:8pt;letter-spacing:3px;text-transform:uppercase;color:#6b7280;}}
</style></head><body>
<div class="cover">
  <div class="glow"></div><div class="glow2"></div><div class="grid"></div>
  <div class="inner">
    <div class="brandrow"><img src="{LOGO}"><span class="bn">loadr<span class="dot">.</span></span></div>
    <div class="kicker">The Performance Engineering Series</div>
    <div class="title">Performance<br>Testing <span class="em">in</span><br>Practice</div>
    <div class="rule"></div>
    <div class="sub">A field guide to load, latency, and scalability — from first test to production confidence.</div>
    <div class="spacer"></div>
    <div class="termcard">
      <span class="p">$</span> <span class="c">loadr</span> run smoke.yaml <span class="o">--slo</span> p99&lt;300ms<br>
      <span class="p">›</span> ramp 0→200 rps · hold 2m · 12,000 reqs<br>
      <span class="g">✓</span> p99 214ms &nbsp;<span class="g">✓</span> error-rate 0.02% &nbsp;<span class="g">PASS</span>
    </div>
    <div class="author">Andy Rea</div>
    <div class="pubrow">Draft Manuscript · loadr.io</div>
  </div>
</div>
</body></html>"""

open(COVER_OUT,"w").write(COVER)

# ---------------------------------------------------------------- body transform
doc = open(RAW).read()

PARTS = {
 1:("Part I","Foundations","The why and the vocabulary"),
 3:("Part II","Designing the Test","Making the test reflect reality"),
 7:("Part III","Executing Tests","Tools, protocols, and scale"),
 11:("Part IV","Observing & Analyzing","Reading the truth in the numbers"),
 15:("Part V","Operationalizing","Making performance continuous"),
}

sect_re = re.compile(r'<div class="sect1">\s*<h2 id="([^"]+)">(.*?)</h2>', re.S)

def opener(m):
    sid, title = m.group(1), m.group(2)
    pre = ""
    cls = ""
    if sid.startswith("ch"):
        n = int(sid[2:])
        num = f"{n:02d}"
        title = re.sub(r'^\d+\.\s+', '', title)  # drop asciidoctor's "N. " (big number shows it)
        if n in PARTS:
            pl, pt, ps = PARTS[n]
            pre = (f'<section class="partdiv"><div class="pd-num">{pl}</div>'
                   f'<div class="pd-rule"></div><h1 class="pd-title">{pt}</h1>'
                   f'<div class="pd-sub">{ps}</div></section>')
        kick = (f'<div class="chapopen"><span class="co-num">{num}</span>'
                f'<span class="co-word">Chapter {n}</span></div>')
    elif sid == "_preface":
        kick = ('<div class="chapopen"><span class="co-num" style="font-size:34pt">§</span>'
                '<span class="co-word">Preface</span></div>')
        cls = " frontish"
    elif sid.startswith("appx-"):
        letter = sid.split("-")[1].upper()
        title = re.sub(r'^Appendix\s+[A-Z]:\s*', '', title)
        kick = (f'<div class="chapopen"><span class="co-num">{letter}</span>'
                f'<span class="co-word">Appendix</span></div>')
    else:
        kick = ""
    return (f'{pre}<div class="sect1{cls}">{kick}<h2 id="{sid}">{title}</h2>')

doc = sect_re.sub(opener, doc)

# Rewrite chapter/appendix cross-reference link TEXT to "Chapter N" / "Appendix X"
# (only inside #content, so the numbered TOC entries are left intact).
head, sep, content = doc.partition('<div id="content">')
if sep:
    content = re.sub(r'<a href="#ch(\d\d)">.*?</a>',
                     lambda m: f'<a href="#ch{m.group(1)}">Chapter {int(m.group(1))}</a>',
                     content, flags=re.S)
    content = re.sub(r'<a href="#appx-([a-c])">.*?</a>',
                     lambda m: f'<a href="#appx-{m.group(1)}">Appendix {m.group(1).upper()}</a>',
                     content, flags=re.S)
    doc = head + sep + content

# inject our stylesheet + fonts + web title
head_inject = f"<style>{CSS}</style>"
if "</head>" in doc:
    doc = doc.replace("</head>", head_inject + "</head>", 1)
else:
    doc = f"<html><head><meta charset='utf-8'>{head_inject}</head><body>{doc}</body></html>"
# ensure a sane <title>
doc = re.sub(r'<title>.*?</title>', '<title>Performance Testing in Practice</title>', doc, flags=re.S)

open(BODY_OUT,"w").write(doc)
print("wrote", COVER_OUT, "and", BODY_OUT)
print("part dividers:", doc.count('class="partdiv"'), "| chapter openers:", doc.count('class="chapopen"'))
