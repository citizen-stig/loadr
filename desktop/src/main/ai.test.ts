import { describe, expect, it, vi } from 'vitest';

import { generatePlan, googleBody, openAiBody } from './ai';

const VALID = '```yaml\nname: t\nscenarios:\n  d:\n    executor: constant-vus\n    vus: 1\n    duration: 5s\n    flow: [ { request: { url: / } } ]\n```';

describe('generatePlan', () => {
  it('returns a valid plan on the first try (no repair)', async () => {
    const chat = vi.fn().mockResolvedValue(VALID);
    const validate = vi.fn().mockResolvedValue({ ok: true, diagnostics: [] });
    const r = await generatePlan({ prompt: 'x' }, chat, validate);
    expect(r.valid).toBe(true);
    expect(r.repaired).toBe(false);
    expect(r.yaml).toContain('constant-vus');
    expect(chat).toHaveBeenCalledOnce();
  });

  it('does one repair round when the first plan is invalid', async () => {
    const chat = vi
      .fn()
      .mockResolvedValueOnce('```yaml\nname: bad\n```')
      .mockResolvedValueOnce(VALID);
    const validate = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, diagnostics: [{ severity: 'error', message: 'no scenarios' }] })
      .mockResolvedValueOnce({ ok: true, diagnostics: [] });
    const r = await generatePlan({ prompt: 'x' }, chat, validate);
    expect(chat).toHaveBeenCalledTimes(2);
    expect(r.repaired).toBe(true);
    expect(r.valid).toBe(true);
    // the repair message must carry the diagnostic back to the model
    expect(chat.mock.calls[1][0].at(-1).content).toContain('no scenarios');
  });

  it('throws when the model returns no YAML', async () => {
    const chat = vi.fn().mockResolvedValue('I cannot help with that.');
    const validate = vi.fn();
    await expect(generatePlan({ prompt: 'x' }, chat, validate)).rejects.toThrow(/did not return/i);
    expect(validate).not.toHaveBeenCalled();
  });
});

describe('provider message mappers', () => {
  it('openAiBody prepends the system message', () => {
    const b = openAiBody('gpt-4o', 'SYS', [{ role: 'user', content: 'hi' }]);
    expect(b.model).toBe('gpt-4o');
    expect(b.messages[0]).toEqual({ role: 'system', content: 'SYS' });
    expect(b.messages[1]).toEqual({ role: 'user', content: 'hi' });
  });

  it('googleBody uses systemInstruction and maps assistant→model', () => {
    const b = googleBody('SYS', [{ role: 'user', content: 'a' }, { role: 'assistant', content: 'b' }]);
    expect(b.systemInstruction.parts[0].text).toBe('SYS');
    expect(b.contents.map((c) => c.role)).toEqual(['user', 'model']);
    expect(b.contents[1].parts[0].text).toBe('b');
  });
});
