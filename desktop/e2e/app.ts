// Shared launcher for the acceptance suite: starts the built Electron app
// against a real display (xvfb in CI) with a resolvable loadr binary, returns
// the window.
//
// NOTE: we deliberately do NOT pass `--ozone-platform=headless`. Chromium's
// headless Ozone backend never opens a window under Xvfb (and segfaults on some
// hosts), so `firstWindow()` would time out. `--no-sandbox` (no SUID sandbox in
// CI) and `--disable-gpu` (no GPU on the runners) are enough; the window then
// renders onto the Xvfb display. This is the same flag set the screenshot
// harness uses successfully.
import { join } from 'node:path';

import { _electron as electron, type ElectronApplication, type Page } from '@playwright/test';

export interface AppHandle {
  app: ElectronApplication;
  page: Page;
}

export async function launchApp(): Promise<AppHandle> {
  const loadrBin = process.env.LOADR_BIN ?? join(process.cwd(), '..', 'target', 'debug', 'loadr');
  const app = await electron.launch({
    args: ['.', '--no-sandbox', '--disable-gpu'],
    env: { ...process.env, LOADR_BIN: loadrBin },
  });
  const page = await app.firstWindow();
  return { app, page };
}
