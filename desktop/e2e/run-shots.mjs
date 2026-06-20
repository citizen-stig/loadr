// Screenshots of an ACTUAL run: stand up a throwaway local server, point a
// plan at it, click Run, and capture live metrics + the results panel.
// Never touches the network.  node e2e/run-shots.mjs
import { createServer } from 'node:http';
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

import { _electron as electron } from '@playwright/test';

const OUT = '/tmp/loadr-shots';
mkdirSync(OUT, { recursive: true });

// Local JSON endpoint the plan will hammer (200 OK so assertions pass).
const server = createServer((_req, res) => {
  res.writeHead(200, { 'content-type': 'application/json' });
  res.end(JSON.stringify({ ok: true, id: 42, token: 'abc123' }));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const port = server.address().port;
console.log('local server on', port);

const loadrBin = process.env.LOADR_BIN ?? join(process.cwd(), '..', 'target', 'debug', 'loadr');
const app = await electron.launch({
  args: ['.', '--no-sandbox', '--disable-gpu'],
  env: { ...process.env, LOADR_BIN: loadrBin },
});
const page = await app.firstWindow();
await page.waitForSelector('text=Desktop');
await page.waitForTimeout(600);

let n = 50;
const shot = async (name) => {
  const file = join(OUT, `${n++}-${name}.png`);
  await page.screenshot({ path: file });
  console.log('shot', file);
};

try {
  // Configure a short, multi-VU run against the local server.
  await page.getByLabel('URL').first().fill(`http://127.0.0.1:${port}/`);
  await page.getByLabel('VUs').fill('10');
  await page.getByLabel('Duration').fill('6s');

  // Add a real assertion (status == 200) so the run exercises checks too.
  await page.getByRole('button', { name: /Assertions, checks & extracts/ }).click();
  await page.getByRole('button', { name: 'Assertion', exact: true }).click();
  await page.waitForTimeout(200);
  await shot('run-configured');

  // Run.
  await page.getByRole('button', { name: /Run/ }).click();
  await page.waitForTimeout(1800);
  await shot('run-live-1');
  await page.waitForTimeout(1800);
  await shot('run-live-2');

  // Wait for the verdict + headline stats, then capture the results panel.
  await page.getByText(/✓ passed|✗ failed/).waitFor({ timeout: 40_000 });
  await page.getByText('thresholds').scrollIntoViewIfNeeded(); // a results pill — errors if pills don't render
  await page.waitForTimeout(400);
  await shot('run-results');

  // Run a second time to populate the history / compare table.
  await page.getByRole('button', { name: /Run/ }).click();
  await page.getByText(/✓ passed|✗ failed/).waitFor({ timeout: 40_000 });
  await page.waitForTimeout(800);
  await page.getByText(/History/).scrollIntoViewIfNeeded();
  await page.waitForTimeout(400);
  await shot('run-history-compare');
} catch (e) {
  console.error('drive error:', e.message);
  await page.screenshot({ path: join(OUT, '98-run-error.png') }).catch(() => {});
} finally {
  await app.close();
  server.close();
  console.log('done');
}
