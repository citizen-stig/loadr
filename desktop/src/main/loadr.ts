// Main-process bridge to the loadr CLI. The GUI is a front-end over the CLI:
// we resolve a bundled, version-pinned binary (falling back to PATH in dev) and
// invoke it with ARRAY args only — never a shell string — so plan content can
// never be interpreted by a shell.

import { execFile, spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';

import { parseSummary, type Summary } from '../shared/results';

const execFileP = promisify(execFile);

/** Where the bundled binary lives inside a packaged app, relative to resources. */
function bundledPath(): string | null {
  // electron sets process.resourcesPath in packaged apps.
  const res = (process as NodeJS.Process & { resourcesPath?: string }).resourcesPath;
  if (!res) return null;
  const exe = process.platform === 'win32' ? 'loadr.exe' : 'loadr';
  const p = join(res, 'bin', exe);
  return existsSync(p) ? p : null;
}

/** Resolve the loadr binary: bundled first, then $LOADR_BIN, then PATH. */
export function resolveLoadr(): string {
  return bundledPath() ?? process.env.LOADR_BIN ?? 'loadr';
}

export interface Diagnostic {
  severity: 'error' | 'warning' | string;
  message: string;
  [k: string]: unknown;
}

export interface ValidateResult {
  ok: boolean; // no errors
  diagnostics: Diagnostic[];
  raw: string;
}

/** `loadr --version`. */
export async function version(): Promise<string> {
  const { stdout } = await execFileP(resolveLoadr(), ['--version']);
  return stdout.trim();
}

/** The plan JSON Schema (`loadr schema`) — drives schema-aware form rendering. */
export async function schema(): Promise<unknown> {
  const { stdout } = await execFileP(resolveLoadr(), ['schema'], { maxBuffer: 32 * 1024 * 1024 });
  return JSON.parse(stdout);
}

/**
 * Validate a plan's YAML by handing it to the CLI. Writes to a temp file (the
 * CLI validates paths) and runs `loadr validate --format json`. Errors are
 * returned as diagnostics, not thrown — invalid plans are an expected state in
 * an editor.
 */
export async function validate(yamlText: string, checkFiles = false): Promise<ValidateResult> {
  const dir = mkdtempSync(join(tmpdir(), 'loadr-validate-'));
  const file = join(dir, 'plan.yaml');
  writeFileSync(file, yamlText);
  const args = ['validate', '--format', 'json'];
  if (!checkFiles) args.push('--no-check-files');
  args.push(file);
  try {
    const { stdout } = await execFileP(resolveLoadr(), args, { maxBuffer: 16 * 1024 * 1024 });
    return parseValidate(stdout);
  } catch (e) {
    // Non-zero exit (validation errors) still carries JSON on stdout.
    const stdout = (e as { stdout?: string }).stdout ?? '';
    if (stdout) return parseValidate(stdout);
    return {
      ok: false,
      diagnostics: [{ severity: 'error', message: (e as Error).message }],
      raw: stdout,
    };
  }
}

/** Map a file extension to a `loadr convert --from` kind. */
export function convertKind(file: string): 'jmx' | 'k6' | 'har' | null {
  const ext = file.split('.').pop()?.toLowerCase();
  if (ext === 'jmx' || ext === 'xml') return 'jmx';
  if (ext === 'js' || ext === 'ts' || ext === 'mjs') return 'k6';
  if (ext === 'har') return 'har';
  return null;
}

/** Import a JMeter/k6/HAR file via `loadr convert`; returns the YAML it emits. */
export async function convert(file: string): Promise<string> {
  const kind = convertKind(file);
  if (!kind) throw new Error(`cannot import ${file}: expected .jmx, .js or .har`);
  const { stdout } = await execFileP(resolveLoadr(), ['convert', '--from', kind, file], {
    maxBuffer: 32 * 1024 * 1024,
  });
  return stdout;
}

/**
 * Run a plan: spawn `loadr run <plan> --summary-export <json>`, stream each
 * progress/log line to `onLine` (loadr uses \r for the live line, so we split
 * on either), and resolve with the parsed end-of-run summary. Array args only.
 */
export function runPlan(yamlText: string, onLine: (line: string) => void): Promise<Summary> {
  const dir = mkdtempSync(join(tmpdir(), 'loadr-run-'));
  const planPath = join(dir, 'plan.yaml');
  const summaryPath = join(dir, 'summary.json');
  writeFileSync(planPath, yamlText);

  return new Promise<Summary>((resolve, reject) => {
    const child = spawn(resolveLoadr(), ['run', planPath, '--summary-export', summaryPath], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const pump = (buf: Buffer) => {
      for (const line of buf.toString().split(/[\r\n]+/)) {
        if (line.trim()) onLine(line);
      }
    };
    child.stdout.on('data', pump);
    child.stderr.on('data', pump);
    child.on('error', reject);
    child.on('close', (code) => {
      try {
        resolve(parseSummary(JSON.parse(readFileSync(summaryPath, 'utf8'))));
      } catch (e) {
        reject(new Error(`run exited (code ${code}) without a summary: ${(e as Error).message}`));
      }
    });
  });
}

function parseValidate(raw: string): ValidateResult {
  let diagnostics: Diagnostic[] = [];
  try {
    const parsed = JSON.parse(raw);
    diagnostics = Array.isArray(parsed) ? parsed : (parsed.diagnostics ?? []);
  } catch {
    /* leave diagnostics empty if the CLI printed non-JSON */
  }
  const ok = !diagnostics.some((d) => d.severity === 'error');
  return { ok, diagnostics, raw };
}
