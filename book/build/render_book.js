// Render the designed book to PDF via headless Chrome (driven by Playwright).
// Paths are configurable via env vars so build.sh can point at a temp dir;
// defaults keep the script runnable standalone.
const path = require('path');
const REPO = path.resolve(__dirname, '..', '..');
const { chromium } = require(process.env.PLAYWRIGHT_PATH ||
  path.join(REPO, 'desktop', 'node_modules', 'playwright'));

const COVER_HTML = process.env.COVER_HTML || '/tmp/cover.html';
const BODY_HTML  = process.env.BODY_HTML  || '/tmp/book_styled.html';
const COVER_PDF  = process.env.COVER_PDF  || '/tmp/cover.pdf';
const BODY_PDF   = process.env.BODY_PDF   || '/tmp/body.pdf';
const CHROME     = process.env.CHROME_PATH || '/usr/bin/google-chrome';

(async () => {
  const browser = await chromium.launch({ executablePath: CHROME, args: ['--no-sandbox'] });
  const page = await browser.newPage();

  // ---- cover: full-bleed, no margins, no header/footer ----
  await page.goto('file://' + COVER_HTML, { waitUntil: 'networkidle' });
  await page.pdf({ path: COVER_PDF, width: '7in', height: '9.25in',
    printBackground: true, margin: { top: '0', bottom: '0', left: '0', right: '0' } });

  // ---- body: page numbers + running title in footer ----
  await page.goto('file://' + BODY_HTML, { waitUntil: 'networkidle' });
  const footer = `<div style="width:100%; font-family:'JetBrainsMono Nerd Font',monospace; font-size:7pt;
     color:#9ca3af; padding:0 0.62in; display:flex; justify-content:space-between; align-items:center;">
     <span style="letter-spacing:1.5px;">PERFORMANCE TESTING IN PRACTICE</span>
     <span style="color:#ef4444; font-weight:700; letter-spacing:1px;"><span class="pageNumber"></span></span></div>`;
  await page.pdf({ path: BODY_PDF, width: '7in', height: '9.25in',
    printBackground: true, displayHeaderFooter: true,
    headerTemplate: '<div></div>', footerTemplate: footer,
    margin: { top: '0.85in', bottom: '0.72in', left: '0.78in', right: '0.78in' } });

  await browser.close();
  console.log('rendered', COVER_PDF, '+', BODY_PDF);
})().catch(e => { console.error(e); process.exit(1); });
