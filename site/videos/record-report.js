// Record a browser session of the self-contained time-series HTML report:
// scroll through the charts and hover them so the shared crosshair + readout
// move. Pass the report path as argv[2] (a file:// URL is built from it).
const { chromium } = require("playwright");
const path = require("path");

const REPORT = process.argv[2] || path.resolve(__dirname, "out/timeseries-report.html");
const URL = "file://" + path.resolve(REPORT);

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1600, height: 900 },
    recordVideo: { dir: "video-out", size: { width: 1600, height: 900 } },
  });
  const page = await context.newPage();
  await page.goto(URL, { waitUntil: "domcontentloaded" });
  await page.waitForTimeout(1500);

  // Settle on the title + status, then reveal the charts.
  await page.waitForTimeout(1200);
  const charts = page.locator("#ts-charts");
  await charts.scrollIntoViewIfNeeded();
  await page.waitForTimeout(1500);

  // Sweep the throughput chart left→right so the crosshair tracks the run.
  async function sweep(selector, holds) {
    const svg = page.locator(selector);
    if (!(await svg.count())) return;
    await svg.scrollIntoViewIfNeeded();
    const box = await svg.boundingBox();
    if (!box) return;
    const y = box.y + box.height / 2;
    const steps = holds || 26;
    for (let i = 0; i <= steps; i++) {
      const x = box.x + 10 + (box.width - 20) * (i / steps);
      await page.mouse.move(x, y);
      await page.waitForTimeout(120);
    }
    await page.waitForTimeout(700);
  }

  // Throughput: watch req/s climb into the spike.
  await sweep('svg[data-chart="throughput"]', 30);
  // Latency: the p99 line jumps during the spike.
  await sweep('svg[data-chart="latency"]', 30);

  // Scroll down to VUs + error charts, sweep VUs (the ramp shape).
  await page.mouse.wheel(0, 280);
  await page.waitForTimeout(1200);
  await sweep('svg[data-chart="vus"]', 30);

  // Down to the aggregate tables — the exact end-of-run figures.
  await page.mouse.wheel(0, 420);
  await page.waitForTimeout(2600);
  await page.mouse.wheel(0, 420);
  await page.waitForTimeout(2400);

  // Back to the top for a clean closing frame.
  await page.mouse.wheel(0, -1400);
  await page.waitForTimeout(1500);

  await context.close(); // flushes the video
  await browser.close();
  console.log("recorded");
})();
