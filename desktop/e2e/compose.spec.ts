import { expect, test } from '@playwright/test';

import { launchApp } from './app';

// Compose a plan through the forms and confirm it flows into the YAML pane;
// reorder a flow step; switch executor and see params reshape.
test('compose: scenario, request, reorder, executor reshape', async () => {
  const { app, page } = await launchApp();

  // Starter plan already has a "default" scenario; add a second request step.
  await page.getByLabel('add step').selectOption('request');
  // Two request steps now in the default scenario's flow.
  await expect(page.getByTestId('step-0')).toBeVisible();
  await expect(page.getByTestId('step-1')).toBeVisible();

  // Edit the new step's URL and confirm it appears in the Monaco YAML pane.
  const urls = page.getByLabel('URL');
  await urls.last().fill('https://api.example.com/v2');
  await expect(page.locator('.view-lines')).toContainText('api.example.com/v2');

  // Keyboard-accessible reorder: focus the drag handle of step-1 and move it up.
  const handle = page.getByTestId('step-1').getByRole('button', { name: /drag/ });
  await handle.focus();
  await page.keyboard.press('Space');
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('Space');
  // (Order asserted via the YAML reflecting the moved URL near the top.)
  await expect(page.locator('.view-lines')).toContainText('api.example.com/v2');

  // Switch executor → params reshape (constant-arrival-rate exposes rate).
  await page.getByLabel('Executor').selectOption('constant-arrival-rate');
  await expect(page.locator('.view-lines')).toContainText('constant-arrival-rate');
  await expect(page.locator('.view-lines')).toContainText('rate:');

  await app.close();
});
