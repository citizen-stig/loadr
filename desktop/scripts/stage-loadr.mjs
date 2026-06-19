// Stage the matching loadr binary into resources/bin before packaging, so the
// app ships with the CLI it drives. Source: $LOADR_BIN, else the workspace
// release build. Run as part of `npm run package`.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

const exe = process.platform === 'win32' ? 'loadr.exe' : 'loadr';
const src = process.env.LOADR_BIN || join('..', 'target', 'release', exe);

if (!existsSync(src)) {
  console.error(`stage-loadr: binary not found at ${src}\n` + 'Build it (cargo build --release -p loadr-cli) or set $LOADR_BIN.');
  process.exit(1);
}

const destDir = join('resources', 'bin');
mkdirSync(destDir, { recursive: true });
const dest = join(destDir, exe);
copyFileSync(src, dest);
console.log(`stage-loadr: ${src} -> ${dest}`);
