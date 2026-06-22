import { describe, expect, it } from 'vitest';

import { buildRepairMessage, buildUserMessage, extractYaml } from './ai';
import { summarizeRepo } from './repo';

describe('extractYaml', () => {
  it('pulls a ```yaml fenced block', () => {
    const out = extractYaml('here you go:\n```yaml\nname: t\nscenarios: {}\n```\nhope it helps');
    expect(out).toBe('name: t\nscenarios: {}');
  });
  it('accepts an unlabelled fenced block', () => {
    expect(extractYaml('```\nname: t\n```')).toBe('name: t');
  });
  it('accepts a bare plan with no fence', () => {
    expect(extractYaml('name: t\nscenarios:\n  d: {}')).toContain('scenarios:');
  });
  it('returns null when there is no plan', () => {
    expect(extractYaml('I cannot do that.')).toBeNull();
  });
});

describe('buildUserMessage', () => {
  it('includes the prompt, schema and the fenced-output instruction', () => {
    const msg = buildUserMessage({ prompt: 'hammer /login', schema: { title: 'Plan' } });
    expect(msg).toContain('hammer /login');
    expect(msg).toContain('JSON Schema');
    expect(msg).toContain('```yaml');
  });
  it('includes repo context when present', () => {
    const repo = summarizeRepo([{ path: 'app.js', content: "app.get('/ping', h)" }]);
    const msg = buildUserMessage({ prompt: 'load test it', repo });
    expect(msg).toContain('GET /ping');
    expect(msg).toContain('repository');
  });
});

describe('buildRepairMessage', () => {
  it('embeds the diagnostics and the previous yaml', () => {
    const msg = buildRepairMessage('name: t', [{ message: 'missing scenarios' }]);
    expect(msg).toContain('missing scenarios');
    expect(msg).toContain('name: t');
    expect(msg).toContain('loadr validate');
  });
});
