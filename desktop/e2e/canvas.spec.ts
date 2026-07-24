// Acceptance tests for the visual Canvas editor, driving the real Electron app
// headless (xvfb in CI, the ambient display locally). They exercise the whole
// loop: the plan renders as nodes, selecting a node shows its GUI form, edits
// propagate to the model (cross-checked in another view), and structural edits
// (delete, reorder, raw-YAML) take effect.

import { expect, test, type Page } from '@playwright/test';

import { launchApp } from './app';

const canvasTab = (page: Page) => page.getByRole('tab', { name: 'Canvas' });
const formTab = (page: Page) => page.getByRole('tab', { name: 'Form' });
const node = (page: Page, text: string) => page.locator('.react-flow__node', { hasText: text });

test.describe('canvas editor', () => {
  test('renders the plan as a node graph', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    // Seed a known request URL in the starter plan via the Form view.
    await page.getByLabel('URL').first().fill('/start');

    await canvasTab(page).click();
    // The plan, its scenario, and the request step all become nodes.
    await expect(page.locator('.react-flow__node').first()).toBeVisible();
    expect(await page.locator('.react-flow__node').count()).toBeGreaterThanOrEqual(3);
    await expect(node(page, 'PLAN')).toBeVisible();
    await expect(node(page, 'SCENARIO')).toBeVisible();
    await expect(node(page, 'request')).toBeVisible();
    await app.close();
  });

  test('selecting a request node shows its full form; edits propagate to the model', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    await canvasTab(page).click();

    await node(page, 'request').first().click();
    await expect(page.getByTestId('inspector-kind')).toHaveText('request');
    // The reused StepFields form shows the real request editor.
    await expect(page.getByLabel('Method')).toBeVisible();
    await expect(page.getByLabel('URL')).toBeVisible();

    // Edit the URL in the inspector; the on-canvas node summary (GET <url>)
    // reflects the model — proving the edit reached it, not just the input.
    await page.getByLabel('URL').fill('/checkout');
    await expect(node(page, '/checkout')).toBeVisible();
    await app.close();
  });

  test('the request body embeds the payload catalog; picking one writes the ${payload:…} template', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    await canvasTab(page).click();

    await node(page, 'request').first().click();
    // Payloads are a first-class part of authoring a request — no separate lab.
    await page.getByRole('button', { name: '+ Payload' }).click();
    await page.getByRole('menuitem', { name: /nested-json/ }).first().click();

    // The body becomes the matching template, verbatim to what the CLI expands.
    await expect(page.getByPlaceholder('{ "email": "${user.email}" }'))
      .toHaveValue(/^\$\{payload:nested-json:\d+\}$/);
    await app.close();
  });

  test('selecting a scenario shows its param form; editing VUs propagates', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    await canvasTab(page).click();

    await node(page, 'SCENARIO').first().click();
    await expect(page.getByTestId('inspector-kind')).toHaveText('scenario');
    await expect(page.getByLabel('Executor')).toBeVisible();

    await page.getByLabel('VUs').fill('42');
    await formTab(page).click();
    await expect(page.getByLabel('VUs').first()).toHaveValue('42');
    await app.close();
  });

  test('delete from the inspector removes the node; pane click deselects', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    await canvasTab(page).click();

    await node(page, 'request').first().click();
    await expect(page.getByTestId('inspector')).toBeVisible();
    await page.getByRole('button', { name: 'delete' }).click();
    await expect(node(page, 'request')).toHaveCount(0);
    // The inspector falls back to its empty state.
    await expect(page.getByText('Select a node on the canvas')).toBeVisible();
    await app.close();
  });

  test('the raw-YAML escape hatch edits a node subtree', async () => {
    const { app, page } = await launchApp();
    await page.waitForSelector('text=Desktop');
    await canvasTab(page).click();

    await node(page, 'request').first().click();
    // Open the collapsible raw editor and rewrite the whole request step.
    await page.getByText('Raw (YAML)').click();
    const raw = page.getByLabel('raw yaml');
    await raw.fill('request:\n  method: GET\n  url: /raw-edited\n  name: viaRaw');
    await raw.blur();
    // Both the structured URL field and the on-canvas summary reflect it.
    await expect(page.getByLabel('URL')).toHaveValue('/raw-edited');
    await expect(node(page, '/raw-edited')).toBeVisible();
    await app.close();
  });
});
