import { describe, expect, it } from 'vitest';

import { defaultModel, getProvider, PROVIDERS } from './providers';

describe('providers', () => {
  it('exposes the big providers with unique ids and at least one model each', () => {
    const ids = PROVIDERS.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toEqual(expect.arrayContaining(['anthropic', 'openai', 'google', 'xai']));
    for (const p of PROVIDERS) {
      expect(p.models.length).toBeGreaterThan(0);
      if (p.transport === 'openai') expect(p.baseUrl).toBeTruthy();
    }
  });

  it('getProvider falls back to a known provider for unknown ids', () => {
    expect(getProvider('openai').id).toBe('openai');
    expect(getProvider('nope').id).toBe('anthropic');
  });

  it('defaultModel returns the first suggested model', () => {
    expect(defaultModel('google')).toBe(getProvider('google').models[0]);
  });
});
