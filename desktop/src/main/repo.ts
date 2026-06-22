// Collect a bounded set of relevant files from a target repo (local path or a
// git URL) and hand them to the pure summarizeRepo. The selection logic prefers
// API-defining files and caps count/size so the prompt stays small.

import { execFile } from 'node:child_process';
import { mkdtemp, readdir, readFile, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { promisify } from 'node:util';

import { summarizeRepo, type RepoContext, type RepoFile } from '../shared/repo';

const execFileP = promisify(execFile);

const SKIP_DIRS = new Set([
  'node_modules', '.git', 'target', 'dist', 'build', 'vendor', '.next', 'out',
  'coverage', '__pycache__', '.venv', 'venv', '.idea', '.vscode',
]);
const KEEP_EXT = /\.(js|ts|jsx|tsx|mjs|cjs|py|go|rb|java|kt|rs|php|ya?ml|json|md)$/i;
const INTERESTING = /openapi|swagger|route|router|controller|handler|api|app|server|main|urls|readme|package\.json|go\.mod|cargo\.toml|requirements\.txt|pyproject|pom\.xml|build\.gradle|gemfile/i;

const MAX_FILES = 60;
const MAX_FILE_BYTES = 200_000;
const MAX_TOTAL_BYTES = 1_500_000;
const MAX_DEPTH = 6;

function isGitUrl(s: string): boolean {
  return /^(https?:\/\/|git@|ssh:\/\/)/.test(s) || s.endsWith('.git');
}

async function listCandidates(root: string): Promise<{ path: string; rel: string; size: number }[]> {
  const found: { path: string; rel: string; size: number }[] = [];
  async function walk(dir: string, depth: number) {
    if (depth > MAX_DEPTH) return;
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name.startsWith('.') && e.name !== '.well-known') {
        if (e.isDirectory()) continue;
      }
      const full = join(dir, e.name);
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name)) await walk(full, depth + 1);
        continue;
      }
      if (!KEEP_EXT.test(e.name)) continue;
      try {
        const s = await stat(full);
        if (s.size > MAX_FILE_BYTES) continue;
        found.push({ path: full, rel: full.slice(root.length + 1), size: s.size });
      } catch {
        /* skip unreadable */
      }
    }
  }
  await walk(root, 0);
  return found;
}

/** Read a bounded, interesting-first selection of files and summarize them. */
export async function gatherRepo(source: string): Promise<RepoContext> {
  let root = source;
  let tmp: string | null = null;
  if (isGitUrl(source)) {
    if (!/^https?:\/\//.test(source)) {
      throw new Error('Only https:// git URLs are supported.');
    }
    tmp = await mkdtemp(join(tmpdir(), 'loadr-repo-'));
    await execFileP('git', ['clone', '--depth', '1', '--quiet', source, tmp], { timeout: 120_000 });
    root = tmp;
  } else {
    const s = await stat(root).catch(() => null);
    if (!s?.isDirectory()) throw new Error(`Not a directory: ${source}`);
  }

  const candidates = await listCandidates(root);
  // interesting files first, then shallower paths, then smaller.
  candidates.sort((a, b) => {
    const ai = INTERESTING.test(a.rel) ? 0 : 1;
    const bi = INTERESTING.test(b.rel) ? 0 : 1;
    if (ai !== bi) return ai - bi;
    const ad = a.rel.split('/').length;
    const bd = b.rel.split('/').length;
    if (ad !== bd) return ad - bd;
    return a.size - b.size;
  });

  const files: RepoFile[] = [];
  let total = 0;
  for (const c of candidates) {
    if (files.length >= MAX_FILES || total + c.size > MAX_TOTAL_BYTES) break;
    try {
      files.push({ path: c.rel, content: await readFile(c.path, 'utf8') });
      total += c.size;
    } catch {
      /* skip */
    }
  }

  const name = basename(source.replace(/\.git$/, '').replace(/\/$/, '')) || 'target';
  return summarizeRepo(files, name);
}
