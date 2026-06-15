// Record a real browsing session of the loadr web UI focused on the failure &
// error breakdown panel, including the browser-side CSV download.
//
// Expects a loadr UI already serving a live (or finished) run that produced a
// mix of failures — see examples/26-failure-breakdown.yaml. Start it on a
// NON-default port so it can run alongside other UI instances:
//
//   loadr run --ui --ui-bind 127.0.0.1:6471 examples/26-failure-breakdown.yaml &
//   node site/videos/record-failure-breakdown.js
//
// The raw frames land in ./video-out (webm); convert with ffmpeg to mp4 +
// poster under site/videos/out/ — see record-failure-breakdown.sh.
// Works with either `playwright` or `playwright-core` (browser must be cached).
let chromium;
try {
  ({ chromium } = require("playwright"));
} catch (_) {
  ({ chromium } = require("playwright-core"));
}
const fs = require("fs");
const path = require("path");

const BASE = process.env.LOADR_UI || "http://127.0.0.1:6471";
const OUT = process.env.VIDEO_OUT || "video-out";
const DOWNLOADS = process.env.DOWNLOAD_DIR || path.join(OUT, "downloads");

(async () => {
  fs.mkdirSync(DOWNLOADS, { recursive: true });
  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    recordVideo: { dir: OUT, size: { width: 1600, height: 900 } },
    acceptDownloads: true,
  });
  const page = await context.newPage();

  // Overview: let the live charts accumulate and failures pile up.
  await page.goto(BASE + "/", { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(8000);

  // Scroll down to the failure breakdown panel.
  const panel = page.locator(".fail-card").first();
  await panel.scrollIntoViewIfNeeded().catch(() => {});
  await page.waitForTimeout(4500);

  // Let the breakdown groups fill in (status / transport / checks / exceptions).
  await page.waitForTimeout(3500);

  // Trigger the CSV download from the panel header and save it.
  const csvBtn = page.locator(".fail-card button", { hasText: /csv/i }).first();
  if (await csvBtn.count()) {
    const dl = page.waitForEvent("download").catch(() => null);
    await csvBtn.click().catch(() => {});
    const download = await dl;
    if (download) {
      const dest = path.join(DOWNLOADS, download.suggestedFilename());
      await download.saveAs(dest).catch(() => {});
      console.log("saved", dest);
    }
    await page.waitForTimeout(2500);
  }

  // Trigger the HTML report download too.
  const htmlBtn = page.locator(".fail-card button", { hasText: /report/i }).first();
  if (await htmlBtn.count()) {
    const dl = page.waitForEvent("download").catch(() => null);
    await htmlBtn.click().catch(() => {});
    const download = await dl;
    if (download) {
      await download
        .saveAs(path.join(DOWNLOADS, download.suggestedFilename()))
        .catch(() => {});
    }
    await page.waitForTimeout(2000);
  }

  // A final settle on the panel.
  await panel.scrollIntoViewIfNeeded().catch(() => {});
  await page.waitForTimeout(3000);

  await context.close(); // flushes the video
  await browser.close();
  console.log("recorded");
})();
