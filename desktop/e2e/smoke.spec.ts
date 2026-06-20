import { expect, test } from '@playwright/test';

import { launchApp } from './app';

test('app launches and renders the workspace shell', async () => {
  const { app, page } = await launchApp();
  await expect(page.getByText('Desktop')).toBeVisible();
  // exact: the outline's "new plan" node is also a button (substring 'new').
  await expect(page.getByRole('button', { name: 'New', exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Plugins', exact: true })).toBeVisible();
  await app.close();
});
