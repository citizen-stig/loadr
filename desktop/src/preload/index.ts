// The only bridge between renderer and main. Exposes a small, typed, allow-
// listed API over contextBridge — no Node, no ipcRenderer, no fs in the renderer.

import { contextBridge, ipcRenderer } from 'electron';

import type { Summary } from '../shared/results';
import type { RunRecord } from '../shared/history';
import type { InstalledPlugin } from '../shared/plugins';
import type { ComplexityResult, PayloadInfo } from '../shared/payload';
import { cleanIpcMessage } from '../shared/ipc';

// Every IPC call goes through here so a rejected main-process handler reaches the
// renderer as a clean, human-readable message — not Electron's
// "Error invoking remote method '…': Error: …" wrapper.
function call<T>(channel: string, ...args: unknown[]): Promise<T> {
  return ipcRenderer.invoke(channel, ...args).catch((e: unknown) => {
    throw new Error(cleanIpcMessage(e instanceof Error ? e.message : String(e)));
  });
}

export interface OpenedPlan {
  path: string;
  content: string;
}

export interface Diagnostic {
  severity: string;
  message: string;
  [k: string]: unknown;
}

export interface ValidateResult {
  ok: boolean;
  diagnostics: Diagnostic[];
  raw: string;
}

export interface RunResult {
  summary: Summary;
  junit: string;
}

export interface Health {
  ok: boolean;
  path: string;
  version?: string;
  problem?: string;
}

export interface LoadrApi {
  version(): Promise<string>;
  doctor(): Promise<Health>;
  schema(): Promise<unknown>;
  validate(yamlText: string): Promise<ValidateResult>;
  openPlan(): Promise<OpenedPlan | null>;
  importPlan(): Promise<OpenedPlan | null>;
  readPlan(path: string): Promise<string>;
  savePlan(path: string | null, content: string): Promise<string | null>;
  run(yamlText: string, onLine: (line: string) => void, onStart?: (runId: string) => void): Promise<RunResult>;
  stopRun(runId: string): Promise<void>;
  saveJunit(content: string): Promise<string | null>;
  historyList(): Promise<RunRecord[]>;
  historyAppend(rec: RunRecord): Promise<RunRecord[]>;
  pluginList(): Promise<InstalledPlugin[]>;
  pluginInstall(spec: string, allowUntrusted: boolean): Promise<string>;
  pluginRemove(name: string): Promise<void>;
  pluginBrowseDir(): Promise<string | null>;
  payloadCatalog(): Promise<PayloadInfo[]>;
  payloadGenerate(kind: string, magnitude: number): Promise<{ bytes: number; preview: string }>;
  payloadComplexity(planPath: string, axis: string, values: number[], maxExponent?: number): Promise<ComplexityResult>;
  ai: {
    hasKey(provider: string): Promise<boolean>;
    setKey(provider: string, key: string): Promise<void>;
    clearKey(provider: string): Promise<void>;
    browseRepo(): Promise<string | null>;
    generate(arg: { provider: string; mode: 'prompt' | 'repo'; prompt: string; source?: string; model: string }): Promise<AiPlanResult>;
  };
}

export interface AiPlanResult {
  yaml: string;
  valid: boolean;
  repaired: boolean;
  diagnostics: { severity: string; message: string }[];
}

const api: LoadrApi = {
  version: () => call('loadr:version'),
  doctor: () => call('loadr:doctor'),
  schema: () => call('loadr:schema'),
  validate: (yamlText) => call('loadr:validate', yamlText),
  openPlan: () => call('plan:open'),
  importPlan: () => call('plan:import'),
  readPlan: (path) => call('plan:read', path),
  savePlan: (path, content) => call('plan:save', path, content),
  run: (yamlText, onLine, onStart) => {
    const runId = `run-${Date.now()}-${Math.round(Math.random() * 1e9)}`;
    onStart?.(runId);
    const listener = (_e: unknown, payload: { runId: string; line: string }) => {
      if (payload.runId === runId) onLine(payload.line);
    };
    ipcRenderer.on('loadr:run:line', listener);
    return call<RunResult>('plan:run', { yaml: yamlText, runId }).finally(() =>
      ipcRenderer.removeListener('loadr:run:line', listener),
    );
  },
  stopRun: (runId) => call('plan:stop', runId),
  saveJunit: (content) => call('report:saveJunit', content),
  historyList: () => call('history:list'),
  historyAppend: (rec) => call('history:append', rec),
  pluginList: () => call('plugin:list'),
  pluginInstall: (spec, allowUntrusted) => call('plugin:install', { spec, allowUntrusted }),
  pluginRemove: (name) => call('plugin:remove', name),
  pluginBrowseDir: () => call('plugin:browseDir'),
  payloadCatalog: () => call('payload:catalog'),
  payloadGenerate: (kind, magnitude) => call('payload:generate', { kind, magnitude }),
  payloadComplexity: (planPath, axis, values, maxExponent) =>
    call('payload:complexity', { planPath, axis, values, maxExponent }),
  ai: {
    hasKey: (provider) => call('ai:hasKey', provider),
    setKey: (provider, key) => call('ai:setKey', { provider, key }),
    clearKey: (provider) => call('ai:clearKey', provider),
    browseRepo: () => call('ai:browseRepo'),
    generate: (arg) => call('ai:generate', arg),
  },
};

contextBridge.exposeInMainWorld('loadr', api);
