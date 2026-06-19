// Shared launcher for the acceptance suite: starts the built Electron app
// headlessly (no display) with a resolvable loadr binary, returns the window.
import { join } from 'node:path';

import { _electron as electron, type ElectronApplication, type Page } from '@playwright/test';

export interface AppHandle {
  app: ElectronApplication;
  page: Page;
}

export async function launchApp(): Promise<AppHandle> {
  const loadrBin = process.env.LOADR_BIN ?? join(process.cwd(), '..', 'target', 'debug', 'loadr');
  const app = await electron.launch({
    args: ['.', '--no-sandbox', '--ozone-platform=headless', '--disable-gpu'],
    env: { ...process.env, LOADR_BIN: loadrBin },
  });
  const page = await app.firstWindow();
  return { app, page };
}
