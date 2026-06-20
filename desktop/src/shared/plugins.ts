// Parse `loadr plugin list` into structured rows for the plugins panel. When the
// CLI is spawned non-interactively its colour output is auto-disabled, so the
// table is plain text. Pure → headless-testable; the spawn lives in main.

export interface InstalledPlugin {
  name: string;
  kind: string;
  type: string;
  state: 'enabled' | 'disabled';
  version: string;
}

// Strip ANSI SGR sequences, INCLUDING the leading ESC (0x1B). The CLI emits
// real `\x1b[1m…\x1b[0m` around the header and `\x1b[32m…` around states; if the
// ESC is left behind the header line no longer starts with "NAME" and leaks in
// as a bogus plugin row. The ESC is optional so plain "[32m" forms strip too.
// eslint-disable-next-line no-control-regex
const ANSI = /\x1b?\[[0-9;]*m/g;

export function parsePluginList(stdout: string): InstalledPlugin[] {
  return stdout
    .replace(ANSI, '')
    .split('\n')
    .map((l) => l.trimEnd())
    .filter(Boolean)
    .filter((l) => !/^NAME\s+KIND/i.test(l) && !/^no plugins/i.test(l))
    .map((l) => l.split(/\s+/))
    .filter((parts) => parts.length >= 5)
    .map(([name, kind, type, state, version]) => ({
      name,
      kind,
      type,
      state: state === 'disabled' ? ('disabled' as const) : ('enabled' as const),
      version,
    }));
}
