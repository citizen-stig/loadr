import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';

import { expect, test } from '@playwright/test';

import { launchApp } from './app';

// Drive the full run loop against a throwaway local server (never the network):
// point the request at it, run, and assert the results panel renders.
test('run a plan and render results', async () => {
  const server = createServer((_req, res) => {
    res.writeHead(200, { 'content-type': 'text/plain' });
    res.end('ok');
  });
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', () => r()));
  const port = (server.address() as AddressInfo).port;

  const { app, page } = await launchApp();
  try {
    await page.getByLabel('URL').first().fill(`http://127.0.0.1:${port}/`);
    await page.getByLabel('Duration').fill('2s');
    await page.getByRole('button', { name: /Run/ }).click();

    // The live monitoring dashboard appears with a verdict and headline tiles.
    await expect(page.getByText(/✓ passed|✗ failed/)).toBeVisible({ timeout: 40_000 });
    await expect(page.getByText('Requests / s', { exact: true })).toBeVisible();
    await expect(page.getByText('Active VUs', { exact: true })).toBeVisible();

    // The run is recorded in history (a compare list appears).
    await expect(page.getByText(/History/)).toBeVisible();
  } finally {
    await app.close();
    server.close();
  }
});
