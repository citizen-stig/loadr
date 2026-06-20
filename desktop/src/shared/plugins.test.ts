import { describe, expect, it } from 'vitest';

import { parsePluginList } from './plugins';

describe('parsePluginList', () => {
  it('parses a list table (header skipped)', () => {
    const out = [
      'NAME                     KIND       TYPE       STATE    VERSION',
      'mongo                    protocol   native     enabled  1.1.0',
      'goecho                   protocol   native     disabled 0.1.0',
    ].join('\n');
    expect(parsePluginList(out)).toEqual([
      { name: 'mongo', kind: 'protocol', type: 'native', state: 'enabled', version: '1.1.0' },
      { name: 'goecho', kind: 'protocol', type: 'native', state: 'disabled', version: '0.1.0' },
    ]);
  });

  it('returns [] for the empty message', () => {
    expect(parsePluginList('no plugins installed in /home/u/.loadr/plugins')).toEqual([]);
  });

  it('tolerates stray ANSI codes', () => {
    const out = 'mongo                    protocol   native     [32menabled[39m  1.1.0';
    const rows = parsePluginList(out);
    expect(rows[0]).toMatchObject({ name: 'mongo', state: 'enabled', version: '1.1.0' });
  });

  it('drops the bold header even when SGR codes keep their ESC byte', () => {
    // Exactly what `loadr plugin list` emits: a real ESC (0x1B) before each [..m.
    const E = String.fromCharCode(27);
    const out = [
      `${E}[1mNAME                    ${E}[0m ${E}[1mKIND      ${E}[0m ${E}[1mTYPE      ${E}[0m ${E}[1mSTATE   ${E}[0m ${E}[1mVERSION${E}[0m`,
      `mongo                    protocol   native     ${E}[32menabled${E}[39m 1.0.0`,
    ].join('\n');
    expect(parsePluginList(out)).toEqual([
      { name: 'mongo', kind: 'protocol', type: 'native', state: 'enabled', version: '1.0.0' },
    ]);
  });
});
