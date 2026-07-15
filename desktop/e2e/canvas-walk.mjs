// Capture the "compose on the canvas" build-up gallery for the marketing site.
//
// Seven control-flow patterns, each shown being built up over three cumulative
// frames. Earlier hand-captured frames were full-window shots dominated by the
// app chrome, so every pattern read as the same picture. This rig fixes that:
// it opens each cumulative plan, CLOSES the inspector so the graph owns the
// full canvas width, fits the view, and screenshots ONLY the React Flow pane —
// so what differs between patterns (the graph shape) is what fills the frame.
//
//   LOADR_BIN=../target/debug/loadr node e2e/canvas-walk.mjs <outDir>
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

import { _electron as electron } from '@playwright/test';

const outArg = process.argv[2];
if (!outArg) {
  console.error('usage: node canvas-walk.mjs <outDir>');
  process.exit(2);
}
const outDir = resolve(outArg);

const loadrBin = process.env.LOADR_BIN
  ? resolve(process.env.LOADR_BIN)
  : join(process.cwd(), '..', 'target', 'debug', 'loadr');

// A scenario wrapper shared by every frame; only the flow differs.
const plan = (name, flow) =>
  `name: ${name}\n` +
  `scenarios:\n` +
  `  default:\n` +
  `    executor: constant-vus\n` +
  `    vus: 10\n` +
  `    duration: 30s\n` +
  `    flow:\n` +
  flow.split('\n').map((l) => (l ? `      ${l}` : l)).join('\n') +
  '\n';

// Seven patterns × three cumulative frames. Each string is the FULL flow at
// that step, so frame N visibly grows out of frame N-1.
const PATTERNS = {
  flat: [
    `- request: { name: browse, url: /products }`,
    `- request: { name: browse, url: /products }
- think_time: { type: constant, duration: 1s }`,
    `- request: { name: browse, url: /products }
- think_time: { type: constant, duration: 1s }
- request: { name: add to cart, method: POST, url: /cart }`,
  ],
  group: [
    `- request: { name: browse, url: /products }`,
    `- request: { name: browse, url: /products }
- group:
    name: checkout
    steps: []`,
    `- request: { name: browse, url: /products }
- group:
    name: checkout
    steps:
      - request: { name: pay, method: POST, url: /pay }`,
  ],
  branch: [
    `- request: { name: login, method: POST, url: /login }`,
    `- request: { name: login, method: POST, url: /login }
- if:
    condition: "session.vars.ok === 'true'"
    then: []
    else: []`,
    `- request: { name: login, method: POST, url: /login }
- if:
    condition: "session.vars.ok === 'true'"
    then:
      - request: { name: dashboard, url: /dashboard }
    else:
      - request: { name: retry login, method: POST, url: /login }`,
  ],
  parallel: [
    `- request: { name: home, url: /home }`,
    `- request: { name: home, url: /home }
- parallel:
    branches:
      - []
      - []`,
    `- request: { name: home, url: /home }
- parallel:
    branches:
      - [ { request: { name: feed, url: /feed } } ]
      - [ { request: { name: inbox, url: /inbox } } ]`,
  ],
  switch: [
    `- request: { name: route, url: /route }`,
    `- request: { name: route, url: /route }
- switch:
    value: "\${vars.plan}"
    cases: {}
    default: []`,
    `- request: { name: route, url: /route }
- switch:
    value: "\${vars.plan}"
    cases:
      free: [ { request: { name: free, url: /free } } ]
      pro:  [ { request: { name: pro, url: /pro } } ]
    default: [ { request: { name: home, url: /home } } ]`,
  ],
  foreach: [
    `- request: { name: list, url: /users }`,
    `- request: { name: list, url: /users }
- foreach:
    items: [ a, b, c ]
    var: user
    steps: []`,
    `- request: { name: list, url: /users }
- foreach:
    items: [ a, b, c ]
    var: user
    steps:
      - request: { name: fetch, url: "/users/\${user}" }`,
  ],
  retry: [
    `- request: { name: submit, method: POST, url: /orders }`,
    `- request: { name: submit, method: POST, url: /orders }
- retry:
    times: 3
    backoff: 1s
    steps: []`,
    `- request: { name: submit, method: POST, url: /orders }
- retry:
    times: 3
    backoff: 1s
    steps:
      - request: { name: charge, method: POST, url: /charge }`,
  ],
};

const settle = (page, ms = 400) => page.waitForTimeout(ms);

// One frame == one fresh app launch. Reusing a window across opens proved
// fragile (a stale/errored editor swallows the view tabs); an isolated launch
// per frame has no such state to leak.
async function frame(slug, idx, yaml) {
  const dir = join(outDir, slug);
  mkdirSync(dir, { recursive: true });
  const file = join(tmpdir(), `loadr-walk-${slug}-${idx}.yaml`);
  writeFileSync(file, yaml);

  const app = await electron.launch({ args: ['.', '--no-sandbox', '--disable-gpu'], env: { ...process.env, LOADR_BIN: loadrBin } });
  try {
    const page = await app.firstWindow();
    await page.setViewportSize({ width: 1280, height: 840 });
    await app.evaluate(async ({ dialog }, p) => {
      dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [p] });
    }, file);

    await page.waitForSelector('text=Desktop');
    await settle(page, 500);
    await page.getByRole('button', { name: 'Open…' }).click();
    await settle(page, 500);
    const closeUntitled = page.getByRole('button', { name: 'close untitled' });
    if (await closeUntitled.count()) await closeUntitled.first().click({ force: true }).catch(() => {});
    await settle(page, 400);

    await page.getByRole('tab', { name: 'Canvas', exact: true }).click();
    await settle(page, 600);
    // Deselect so the inspector closes and the graph owns the full width.
    await page.locator('.react-flow__pane').click({ position: { x: 300, y: 120 } }).catch(() => {});
    await settle(page, 200);
    const fit = page.locator('.react-flow__controls-fitview');
    if (await fit.count()) { await fit.first().click().catch(() => {}); await settle(page, 400); }
    await page.mouse.move(20, 20); // park off any node — no hover cards
    await settle(page, 300);

    // Screenshot ONLY the canvas pane: the graph is the whole image.
    await page.locator('.react-flow').first().screenshot({ path: join(dir, `${idx}.png`) });
    console.log('  shot', slug, idx);
  } finally {
    await app.close();
  }
}

for (const [slug, frames] of Object.entries(PATTERNS)) {
  for (let i = 0; i < frames.length; i++) await frame(slug, i, plan(slug, frames[i]));
}
writeFileSync(join(outDir, 'steps.json'), JSON.stringify({ patterns: Object.keys(PATTERNS) }, null, 2));
console.log('done ->', outDir);
