// Electron main process: one window, secure defaults, and an IPC surface that
// the renderer reaches only through the typed preload bridge. The renderer
// never spawns processes or touches the filesystem directly.

import { existsSync } from 'node:fs';
import { readFile, unlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

import { app, BrowserWindow, dialog, ipcMain, safeStorage, type IpcMainInvokeEvent } from 'electron';
import type { ChildProcess } from 'node:child_process';

import {
  convert, pluginInstall, pluginList, pluginRemove, runPlan, schema, validate, version,
} from './loadr';
import { generatePlan, providerChat, type ChatMessage } from './ai';
import { gatherRepo } from './repo';
import { getProvider, type ProviderId } from '../shared/providers';
import { addRun, type RunRecord } from '../shared/history';

const isDev = !app.isPackaged;

function createWindow(): void {
  const win = new BrowserWindow({
    width: 1280,
    height: 860,
    backgroundColor: '#07070a',
    show: false,
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  win.once('ready-to-show', () => win.show());

  if (isDev && process.env.ELECTRON_RENDERER_URL) {
    void win.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    void win.loadFile(join(__dirname, '../renderer/index.html'));
  }
}

// ---- IPC: loadr CLI -------------------------------------------------------
ipcMain.handle('loadr:version', () => version());
ipcMain.handle('loadr:schema', () => schema());
ipcMain.handle('loadr:validate', (_e: IpcMainInvokeEvent, yamlText: string) => validate(yamlText));

// ---- IPC: run -------------------------------------------------------------
// Track in-flight runs by id so the renderer can stop one mid-flight.
const runningChildren = new Map<string, ChildProcess>();
ipcMain.handle('plan:run', (event: IpcMainInvokeEvent, arg: { yaml: string; runId: string }) =>
  runPlan(
    arg.yaml,
    (line) => event.sender.send('loadr:run:line', { runId: arg.runId, line }),
    (child) => {
      runningChildren.set(arg.runId, child);
      child.on('close', () => runningChildren.delete(arg.runId));
    },
  ),
);
// Stop a run with SIGINT so loadr shuts down gracefully and still flushes its
// summary export (a Ctrl-C equivalent), rather than a hard kill.
ipcMain.handle('plan:stop', (_e: IpcMainInvokeEvent, runId: string) => {
  runningChildren.get(runId)?.kill('SIGINT');
});

// ---- IPC: run history (persisted in userData) ------------------------------
const historyFile = () => join(app.getPath('userData'), 'run-history.json');
async function readHistory(): Promise<RunRecord[]> {
  try {
    return JSON.parse(await readFile(historyFile(), 'utf8'));
  } catch {
    return [];
  }
}
ipcMain.handle('history:list', () => readHistory());
ipcMain.handle('history:append', async (_e: IpcMainInvokeEvent, rec: RunRecord) => {
  const next = addRun(await readHistory(), rec);
  await writeFile(historyFile(), JSON.stringify(next), 'utf8');
  return next;
});

// ---- IPC: plugins ---------------------------------------------------------
ipcMain.handle('plugin:list', () => pluginList());
ipcMain.handle('plugin:install', (_e: IpcMainInvokeEvent, a: { spec: string; allowUntrusted: boolean }) =>
  pluginInstall(a.spec, a.allowUntrusted),
);
ipcMain.handle('plugin:remove', (_e: IpcMainInvokeEvent, name: string) => pluginRemove(name));
ipcMain.handle('plugin:browseDir', async () => {
  const r = await dialog.showOpenDialog({ properties: ['openDirectory'] });
  return r.canceled || r.filePaths.length === 0 ? null : r.filePaths[0];
});

// ---- IPC: files -----------------------------------------------------------
ipcMain.handle('plan:open', async () => {
  const r = await dialog.showOpenDialog({
    properties: ['openFile'],
    filters: [{ name: 'loadr plan', extensions: ['yaml', 'yml'] }],
  });
  if (r.canceled || r.filePaths.length === 0) return null;
  const path = r.filePaths[0];
  const content = await readFile(path, 'utf8');
  return { path, content };
});

ipcMain.handle('plan:read', (_e: IpcMainInvokeEvent, path: string) => readFile(path, 'utf8'));

ipcMain.handle('plan:import', async () => {
  const r = await dialog.showOpenDialog({
    properties: ['openFile'],
    filters: [
      { name: 'Importable', extensions: ['jmx', 'js', 'ts', 'mjs', 'har', 'xml'] },
      { name: 'JMeter', extensions: ['jmx', 'xml'] },
      { name: 'k6', extensions: ['js', 'ts', 'mjs'] },
      { name: 'HAR', extensions: ['har'] },
    ],
  });
  if (r.canceled || r.filePaths.length === 0) return null;
  const path = r.filePaths[0];
  const content = await convert(path);
  return { path, content };
});

ipcMain.handle('plan:save', async (_e: IpcMainInvokeEvent, path: string | null, content: string) => {
  let target = path;
  if (!target) {
    const r = await dialog.showSaveDialog({
      filters: [{ name: 'loadr plan', extensions: ['yaml'] }],
    });
    if (r.canceled || !r.filePath) return null;
    target = r.filePath;
  }
  await writeFile(target, content, 'utf8');
  return target;
});

// ---- IPC: AI plan authoring ------------------------------------------------
// The Anthropic API key is stored OS-encrypted (safeStorage) in userData; it is
// never exposed back to the renderer. All network/LLM calls happen here in main.
// One encrypted key file per provider. getProvider() sanitises the id (unknown
// ids fall back to a known one), so the path can't be influenced by the renderer.
const keyFile = (provider: string) => join(app.getPath('userData'), `ai-key-${getProvider(provider).id}.bin`);

async function setApiKey(provider: string, key: string): Promise<void> {
  const blob = safeStorage.isEncryptionAvailable()
    ? safeStorage.encryptString(key)
    : Buffer.from(`plain:${key}`);
  await writeFile(keyFile(provider), blob);
}
async function getApiKey(provider: string): Promise<string | null> {
  let buf: Buffer;
  try {
    buf = await readFile(keyFile(provider));
  } catch {
    return null;
  }
  try {
    if (safeStorage.isEncryptionAvailable()) return safeStorage.decryptString(buf);
  } catch {
    /* fall through to plain */
  }
  const s = buf.toString();
  return s.startsWith('plain:') ? s.slice(6) : null;
}

let schemaCache: unknown;
async function cachedSchema(): Promise<unknown> {
  if (!schemaCache) schemaCache = await schema();
  return schemaCache;
}

ipcMain.handle('ai:hasKey', (_e: IpcMainInvokeEvent, provider: string) => existsSync(keyFile(provider)));
ipcMain.handle('ai:setKey', (_e: IpcMainInvokeEvent, a: { provider: string; key: string }) => setApiKey(a.provider, a.key));
ipcMain.handle('ai:clearKey', async (_e: IpcMainInvokeEvent, provider: string) => {
  await unlink(keyFile(provider)).catch(() => {});
});
ipcMain.handle('ai:browseRepo', async () => {
  const r = await dialog.showOpenDialog({ properties: ['openDirectory'] });
  return r.canceled || r.filePaths.length === 0 ? null : r.filePaths[0];
});
ipcMain.handle(
  'ai:generate',
  async (
    _e: IpcMainInvokeEvent,
    arg: { provider: ProviderId; mode: 'prompt' | 'repo'; prompt: string; source?: string; model: string },
  ) => {
    const apiKey = await getApiKey(arg.provider);
    if (!apiKey) throw new Error(`Set your ${getProvider(arg.provider).label} API key first.`);
    const repo = arg.mode === 'repo' && arg.source ? await gatherRepo(arg.source) : null;
    const chat = (messages: ChatMessage[]) => providerChat(arg.provider, apiKey, arg.model, messages);
    const validateFn = async (yaml: string) => {
      const v = await validate(yaml);
      return { ok: v.ok, diagnostics: v.diagnostics };
    };
    return generatePlan({ prompt: arg.prompt, schema: await cachedSchema(), repo }, chat, validateFn);
  },
);

app.whenReady().then(createWindow);
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
app.on('activate', () => {
  if (BrowserWindow.getAllWindows().length === 0) createWindow();
});
