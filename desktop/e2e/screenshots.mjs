// Drives the built desktop app through its UI and saves screenshots of every
// step kind's form plus the outline / views / plugins. One controlled Electron
// instance (no window spam): launch, drive, shoot, close.
//   node e2e/screenshots.mjs
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';

import { _electron as electron } from '@playwright/test';

const OUT = '/tmp/loadr-shots';
mkdirSync(OUT, { recursive: true });

const loadrBin = process.env.LOADR_BIN ?? join(process.cwd(), '..', 'target', 'debug', 'loadr');
const app = await electron.launch({
  args: ['.', '--no-sandbox', '--disable-gpu'],
  env: { ...process.env, LOADR_BIN: loadrBin },
});
const page = await app.firstWindow();
await page.waitForSelector('text=Desktop');
await page.waitForTimeout(600);

let n = 0;
const shot = async (name) => {
  const file = join(OUT, `${String(n++).padStart(2, '0')}-${name}.png`);
  await page.screenshot({ path: file });
  console.log('shot', file);
};
const addTop = (kind) => page.getByLabel('add step').first().selectOption(kind);
const removeStep1 = () => page.getByTestId('step-1').getByRole('button', { name: 'remove step' }).first().click();
const clickBtn = (name) => page.getByRole('button', { name, exact: true }).click();

try {
  // 0 — default forms-first view with the new outline rail
  await page.getByLabel('URL').first().fill('https://api.example.com/login');
  await page.waitForTimeout(150);
  await shot('overview-outline');

  // --- one screenshot per step kind's form -------------------------------
  // 'request' is already shown by the starter step; walk the rest.
  const kinds = [
    'think_time', 'js', 'group', 'repeat', 'while', 'if',
    'foreach', 'switch', 'during', 'retry', 'parallel', 'random', 'rendezvous',
  ];
  for (const kind of kinds) {
    await addTop(kind);
    await page.waitForTimeout(150);
    // enrich container kinds so their nested editors are visible
    if (kind === 'switch') await clickBtn('Case');
    if (kind === 'parallel') { await clickBtn('Branch'); await clickBtn('Branch'); }
    if (kind === 'random') await clickBtn('Choice');
    await page.waitForTimeout(150);
    await page.getByTestId('step-1').scrollIntoViewIfNeeded();
    await page.waitForTimeout(150);
    await shot(`kind-${kind}`);
    await removeStep1();
    await page.waitForTimeout(100);
  }

  // --- compose a nested plan to populate the outline ---------------------
  await addTop('foreach');
  await page.waitForTimeout(150);
  await page.getByLabel('Items').fill('${users}');
  await page.getByTestId('step-1').getByLabel('add step').selectOption('request');
  await page.waitForTimeout(150);
  await page.getByTestId('step-1').getByLabel('URL').fill('/users/${item}/profile');

  await addTop('if');
  await page.waitForTimeout(150);
  await page.getByTestId('step-2').getByLabel('add step').first().selectOption('request');
  await page.waitForTimeout(150);

  await addTop('parallel');
  await page.waitForTimeout(150);
  await clickBtn('Branch');
  await clickBtn('Branch');
  await page.waitForTimeout(200);

  // scroll back to top, capture the populated outline + forms
  await page.getByText('Plan', { exact: true }).scrollIntoViewIfNeeded();
  await page.waitForTimeout(200);
  await shot('outline-populated');

  // --- outline navigation: click a node, the card highlights + scrolls ---
  const nav = page.getByRole('navigation', { name: 'plan outline' });
  await nav.getByText('foreach', { exact: true }).click();
  await page.waitForTimeout(500);
  await shot('outline-navigate');

  // --- optional YAML views -----------------------------------------------
  await page.getByRole('tab', { name: 'Split' }).click();
  await page.waitForSelector('.view-lines');
  await page.waitForTimeout(500);
  await shot('split-view');

  await page.getByRole('tab', { name: 'YAML' }).click();
  await page.waitForTimeout(500);
  await shot('yaml-view');

  // --- plugins (parser fix: no bogus header row) -------------------------
  await page.getByRole('tab', { name: 'Form' }).click();
  await page.getByRole('button', { name: 'Plugins' }).click();
  await page.waitForTimeout(400);
  await shot('plugins-fixed');
  await page.getByLabel('close plugins').click();
} catch (e) {
  console.error('drive error:', e.message);
  await page.screenshot({ path: join(OUT, '99-error-state.png') }).catch(() => {});
} finally {
  await app.close();
  console.log('done:', n, 'screenshots in', OUT);
}
