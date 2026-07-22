// Capture a desktop walkthrough of one loadr demo plan.
//
// Launches the built Electron app, "opens" the plan (via a stubbed native
// dialog so no human picks a file), and drives it through every transition —
// the plan loaded, the canvas graph, a node inspector, the YAML, a LIVE run
// against the running backend, the pass/fail results, and the run history —
// saving one screenshot per transition plus a steps.json manifest.
//
//   BASE_URL=http://localhost:8081 LOADR_BIN=../target/debug/loadr \
//     node e2e/demo-walkthroughs.mjs <plan.yaml> <outDir> <demoName>
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { _electron as electron } from '@playwright/test';

const [planArg, outArg, demoName] = process.argv.slice(2);
if (!planArg || !outArg) {
  console.error('usage: node demo-walkthroughs.mjs <plan.yaml> <outDir> <demoName>');
  process.exit(2);
}
const planPath = resolve(planArg);
const outDir = resolve(outArg);
mkdirSync(outDir, { recursive: true });

const loadrBin = process.env.LOADR_BIN
  ? resolve(process.env.LOADR_BIN)
  : join(process.cwd(), '..', 'target', 'debug', 'loadr');

const app = await electron.launch({
  args: ['.', '--no-sandbox', '--disable-gpu'],
  env: { ...process.env, LOADR_BIN: loadrBin },
});
const page = await app.firstWindow();

// Make the app's own "Open…" resolve to our plan — a real open, no file picker.
await app.evaluate(async ({ dialog }, p) => {
  dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [p] });
}, planPath);

const steps = [];
let n = 0;
const shot = async (slug) => {
  const file = `${String(n++).padStart(2, '0')}-${slug}.png`;
  await page.screenshot({ path: join(outDir, file) });
  steps.push({ slug, file });
  console.log('  shot', file);
};
const settle = (ms = 500) => page.waitForTimeout(ms);
// After the untitled tab is closed only one Editor remains, so these are unambiguous.
// exact:true — the plan's tab title ("smoke.yaml") otherwise collides with the "YAML" view tab.
const viewTab = (name) => page.getByRole('tab', { name, exact: true });
const parkMouse = () => page.mouse.move(560, 200); // off any node, kills hover cards
const node = (text) => page.locator('.react-flow__node', { hasText: text });

try {
  await page.waitForSelector('text=Desktop');
  await settle(600);

  // Open the demo plan, then drop the starter "untitled" tab.
  await page.getByRole('button', { name: 'Open…' }).click();
  await settle(700);
  const closeUntitled = page.getByRole('button', { name: 'close untitled' });
  if (await closeUntitled.count()) await closeUntitled.first().click({ force: true });
  await settle(500);

  // Canvas is the home view for every walkthrough — lead with the node graph.
  const fitView = async () => {
    const fit = page.locator('.react-flow__controls-fitview');
    if (await fit.count()) { await fit.first().click().catch(() => {}); await settle(500); }
  };

  // 1 — the whole plan as a drag-and-drop node graph.
  await viewTab('Canvas').click();
  await settle(700);
  await fitView();
  await parkMouse();
  await settle(300);
  await shot('canvas');

  // 2 — inspect a request node on the canvas: its full GUI form.
  let target = node('request').first();
  if (!(await target.count())) target = page.locator('.react-flow__node').last();
  await target.click().catch(() => {});
  await settle(500);
  await parkMouse();
  await settle(300);
  await shot('inspect-request');

  // 3 — inspect the scenario node: the workload shape (executor / VUs / duration).
  const scenario = node('SCENARIO').first();
  if (await scenario.count()) {
    await scenario.click().catch(() => {});
    await settle(500);
    await parkMouse();
    await settle(300);
    await shot('inspect-scenario');
  }

  // 4 — the exact YAML, always in sync with the canvas.
  await viewTab('YAML').click();
  await page.waitForSelector('.view-lines').catch(() => {});
  await settle(600);
  await shot('yaml');

  // 5 — back on the canvas, run it LIVE against the backend; graph stays in view.
  await viewTab('Canvas').click();
  await settle(400);
  // Deselect (close the inspector) so the graph uses the full canvas width.
  await page.locator('.react-flow__pane').click({ position: { x: 300, y: 120 } }).catch(() => {});
  await settle(300);
  await page.getByRole('button', { name: /Run/ }).click();
  await settle(2200);
  // The run panel now occupies the lower half — re-fit the graph into what's left.
  await fitView();
  await parkMouse();
  await settle(300);
  await shot('run-live');

  // 6 — the verdict + results panel once the run lands.
  await page.getByText(/✓ passed|✗ failed/).waitFor({ timeout: 120_000 });
  await settle(600);
  await shot('results');

  // 7 — a second run populates history / compare.
  try {
    await page.getByRole('button', { name: /Run/ }).click();
    await page.getByText(/✓ passed|✗ failed/).waitFor({ timeout: 120_000 });
    await settle(700);
    await page.getByText(/History/).scrollIntoViewIfNeeded();
    await settle(400);
    await shot('history');
  } catch (e) {
    console.log('  (history step skipped:', e.message, ')');
  }

  writeFileSync(join(outDir, 'steps.json'), JSON.stringify({ demo: demoName, steps }, null, 2));
  console.log('done:', n, 'shots ->', outDir);
} catch (e) {
  console.error('capture error:', e.message);
  await page.screenshot({ path: join(outDir, '99-error.png') }).catch(() => {});
  writeFileSync(join(outDir, 'steps.json'), JSON.stringify({ demo: demoName, steps, error: e.message }, null, 2));
  process.exitCode = 1;
} finally {
  await app.close();
}
