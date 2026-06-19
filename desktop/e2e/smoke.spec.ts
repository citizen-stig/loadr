import { expect, test } from '@playwright/test';

import { launchApp } from './app';

test('app launches and renders the workspace shell', async () => {
  const { app, page } = await launchApp();
  await expect(page.getByText('Desktop')).toBeVisible();
  await expect(page.getByRole('button', { name: 'New' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Plugins' })).toBeVisible();
  await app.close();
});
