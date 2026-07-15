// Capture the "convert" walkthrough: Import… a k6 script and watch it land on
// the canvas as a native loadr plan (loadr convert under the hood). No run —
// the story is the conversion itself.
//   node e2e/convert-walkthrough.mjs <source.js> <outDir>
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { _electron as electron } from '@playwright/test';

const [srcArg, outArg] = process.argv.slice(2);
const srcPath = resolve(srcArg);
const outDir = resolve(outArg);
mkdirSync(outDir, { recursive: true });

const loadrBin = process.env.LOADR_BIN ?? join(process.cwd(), '..', 'target', 'debug', 'loadr');
const app = await electron.launch({
  args: ['.', '--no-sandbox', '--disable-gpu'],
  env: { ...process.env, LOADR_BIN: loadrBin },
});
const page = await app.firstWindow();
await app.evaluate(async ({ dialog }, p) => {
  dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [p] });
}, srcPath);

const steps = [];
let n = 0;
const shot = async (slug) => {
  const file = `${String(n++).padStart(2, '0')}-${slug}.png`;
  await page.screenshot({ path: join(outDir, file) });
  steps.push({ slug, file });
  console.log('  shot', file);
};
const settle = (ms = 500) => page.waitForTimeout(ms);
const viewTab = (name) => page.getByRole('tab', { name, exact: true });

try {
  await page.waitForSelector('text=Desktop');
  await settle(600);

  // 1 — the source: a k6 script, before conversion (plain title card of the app).
  await shot('before-import');

  // 2 — Import…: loadr convert runs under the hood, a new tab opens with the plan.
  await page.getByRole('button', { name: 'Import…' }).click();
  await settle(1200);
  const closeUntitled = page.getByRole('button', { name: 'close untitled' });
  if (await closeUntitled.count()) await closeUntitled.first().click({ force: true });
  await settle(400);
  await shot('imported-forms');

  // 3 — the converted plan as a canvas graph.
  await viewTab('Canvas').click();
  await settle(700);
  const fit = page.locator('.react-flow__controls-fitview');
  if (await fit.count()) { await fit.first().click().catch(() => {}); await settle(500); }
  await page.mouse.move(560, 200);
  await settle(300);
  await shot('canvas');

  // 4 — the generated YAML: what `loadr convert` produced.
  await viewTab('YAML').click();
  await page.waitForSelector('.view-lines').catch(() => {});
  await settle(600);
  await shot('yaml');

  writeFileSync(join(outDir, 'steps.json'), JSON.stringify({ demo: 'convert', steps }, null, 2));
  console.log('done:', n, 'shots ->', outDir);
} catch (e) {
  console.error('capture error:', e.message);
  await page.screenshot({ path: join(outDir, '99-error.png') }).catch(() => {});
  process.exitCode = 1;
} finally {
  await app.close();
}
