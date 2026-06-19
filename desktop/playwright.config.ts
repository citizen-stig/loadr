import { defineConfig } from '@playwright/test';

// Playwright-for-Electron acceptance tests. They launch the built app
// (out/) and drive the renderer DOM. Headless via Electron's `--ozone-platform=
// headless` (no xvfb needed); in CI the same applies on the Linux matrix leg.
export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
});
