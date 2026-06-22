// The AI providers the desktop app can use for plan generation. Pure data +
// helpers (renderer-safe); the actual HTTP transports live in main/ai.ts and
// switch on `transport`. Model lists are *suggestions* — the UI lets you type
// any model, so this never goes stale.

export type ProviderId = 'anthropic' | 'openai' | 'google' | 'xai';
export type Transport = 'anthropic' | 'openai' | 'google';

export interface Provider {
  id: ProviderId;
  label: string;
  transport: Transport; // openai = OpenAI-compatible /chat/completions
  baseUrl?: string; // for openai-compatible transports
  models: string[]; // suggested models (first = default)
  keyHint: string; // placeholder for the key field
  keysUrl: string; // where to get a key
}

export const PROVIDERS: Provider[] = [
  {
    id: 'anthropic',
    label: 'Anthropic — Claude',
    transport: 'anthropic',
    models: ['claude-sonnet-4-6', 'claude-opus-4-8', 'claude-haiku-4-5-20251001'],
    keyHint: 'sk-ant-…',
    keysUrl: 'https://console.anthropic.com/settings/keys',
  },
  {
    id: 'openai',
    label: 'OpenAI — GPT',
    transport: 'openai',
    baseUrl: 'https://api.openai.com/v1',
    models: ['gpt-4o', 'gpt-4.1', 'gpt-4o-mini', 'o4-mini'],
    keyHint: 'sk-…',
    keysUrl: 'https://platform.openai.com/api-keys',
  },
  {
    id: 'google',
    label: 'Google — Gemini',
    transport: 'google',
    models: ['gemini-2.5-pro', 'gemini-2.5-flash', 'gemini-2.0-flash'],
    keyHint: 'AIza…',
    keysUrl: 'https://aistudio.google.com/apikey',
  },
  {
    id: 'xai',
    label: 'xAI — Grok',
    transport: 'openai',
    baseUrl: 'https://api.x.ai/v1',
    models: ['grok-4', 'grok-3', 'grok-2-latest'],
    keyHint: 'xai-…',
    keysUrl: 'https://console.x.ai',
  },
];

const BY_ID: Record<string, Provider> = Object.fromEntries(PROVIDERS.map((p) => [p.id, p]));

export function getProvider(id: string): Provider {
  return BY_ID[id] ?? PROVIDERS[0];
}

export function defaultModel(id: string): string {
  return getProvider(id).models[0];
}
