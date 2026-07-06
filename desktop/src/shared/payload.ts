// The adversarial-payload catalog and complexity-probe types, shared by main and
// renderer. The catalog is hard-coded here (mirroring the authoritative
// `CATALOG` in crates/loadr-payload/src/lib.rs) rather than parsed from
// `loadr payload --list`: the pretty table is fragile to scrape, and a static
// list needs no subprocess. Keep this file PURE (no node imports) so the
// renderer can bundle it and the fit is headless-testable.

/** Static description of one payload kind — powers the Payload Lab catalog. */
export interface PayloadInfo {
  /** Spec name (the part before the `:`). */
  name: string;
  /** Grouping for listings (nesting, amplification, volume, regex, …). */
  category: string;
  /** What the magnitude controls: depth / count / bytes / levels. */
  param: string;
  /** Magnitude used when the spec omits `:n`. */
  default: number;
  /** Hard safety cap on the magnitude. */
  max: number;
  /** Suggested `Content-Type` for a request carrying this body. */
  contentType: string;
  /** One-line description of what it stresses. */
  about: string;
}

/** The full catalog of payload kinds (mirrors loadr-payload's `CATALOG`). */
export const PAYLOAD_CATALOG: PayloadInfo[] = [
  // ---- nesting: deep structure → super-linear parsers ---------------------
  { name: 'nested-json', category: 'nesting', param: 'depth', default: 10_000, max: 5_000_000, contentType: 'application/json',
    about: 'Deeply nested JSON object {"a":{"a":…}} — stresses recursive-descent / stack-depth parsing.' },
  { name: 'nested-array', category: 'nesting', param: 'depth', default: 10_000, max: 5_000_000, contentType: 'application/json',
    about: 'Deeply nested JSON array [[[…]]] — same parser stress via array nesting.' },
  { name: 'nested-markdown-blockquote', category: 'nesting', param: 'depth', default: 50_000, max: 5_000_000, contentType: 'text/markdown',
    about: 'One line of N blockquote markers (>>>…) — the goldmark-class super-quadratic blowup.' },
  { name: 'nested-markdown-bracket', category: 'nesting', param: 'depth', default: 50_000, max: 5_000_000, contentType: 'text/markdown',
    about: 'Unmatched nested link brackets [[[…]]] — stresses inline link/reference backtracking.' },
  { name: 'nested-xml', category: 'nesting', param: 'depth', default: 20_000, max: 5_000_000, contentType: 'application/xml',
    about: 'Deeply nested XML elements <a><a>…</a></a> — stack/tree-depth parser stress.' },
  { name: 'nested-html', category: 'nesting', param: 'depth', default: 20_000, max: 5_000_000, contentType: 'text/html',
    about: 'Deeply nested <div> tags — stresses HTML parsers and sanitizers walking a deep tree.' },
  { name: 'nested-parens', category: 'nesting', param: 'depth', default: 50_000, max: 5_000_000, contentType: 'text/plain',
    about: 'Balanced nested parentheses ((((…)))) — stresses expression/formula/filter grammars.' },
  { name: 'nested-graphql', category: 'nesting', param: 'depth', default: 2_000, max: 200_000, contentType: 'application/json',
    about: 'Deeply nested GraphQL selection {a{a{…}}} — stresses query validation / depth limiting.' },
  // ---- amplification: small in → huge out ---------------------------------
  { name: 'billion-laughs', category: 'amplification', param: 'levels', default: 9, max: 12, contentType: 'application/xml',
    about: 'Classic XML entity-expansion bomb — ~10^levels expansion from a tiny document.' },
  { name: 'yaml-alias-bomb', category: 'amplification', param: 'levels', default: 10, max: 24, contentType: 'application/x-yaml',
    about: 'Exponential YAML anchor/alias expansion (&a […] then [*a,*a] …) — 2^levels blowup.' },
  // ---- volume: allocation / O(n^2) stress ---------------------------------
  { name: 'json-array', category: 'volume', param: 'count', default: 1_000_000, max: 50_000_000, contentType: 'application/json',
    about: 'A flat JSON array of N integers — allocation, GC and per-element processing stress.' },
  { name: 'json-object-keys', category: 'volume', param: 'count', default: 1_000_000, max: 20_000_000, contentType: 'application/json',
    about: 'A JSON object with N distinct keys — hashmap-build and key-processing stress.' },
  { name: 'long-string', category: 'volume', param: 'bytes', default: 10_000_000, max: 200_000_000, contentType: 'application/json',
    about: 'A single JSON string of N bytes — copy/scan/validation cost in one enormous field.' },
  { name: 'csv-rows', category: 'volume', param: 'count', default: 1_000_000, max: 50_000_000, contentType: 'text/csv',
    about: 'A CSV with N rows — row-parsing throughput and streaming behaviour.' },
  // ---- regex: catastrophic backtracking (ReDoS) ---------------------------
  { name: 'redos', category: 'regex', param: 'bytes', default: 50_000, max: 10_000_000, contentType: 'text/plain',
    about: "'aaaa…!' — drives (a+)+$-style vulnerable validators into exponential backtracking." },
  // ---- unicode: normalization / grapheme cost -----------------------------
  { name: 'zalgo', category: 'unicode', param: 'count', default: 100_000, max: 20_000_000, contentType: 'text/plain',
    about: 'A base char with N stacked combining marks — normalization / width / grapheme cost.' },
  // ---- numeric: slow number parsing ---------------------------------------
  { name: 'bignum', category: 'numeric', param: 'count', default: 100_000, max: 50_000_000, contentType: 'application/json',
    about: 'A bare integer with N digits — bignum / arbitrary-precision parse cost.' },
  // ---- collision: worst-case hashmaps -------------------------------------
  { name: 'hash-collision', category: 'collision', param: 'count', default: 65_536, max: 1_000_000, contentType: 'application/json',
    about: 'A JSON object whose N keys all collide in 31-based string hashing — O(n^2) map inserts.' },
];

/** One measured (input size, response latency) point on the complexity axis. */
export interface ComplexityPoint {
  size: number;
  latencyMs: number;
}

/** Result of a `loadr sweep --complexity` probe, fitted client-side. */
export interface ComplexityResult {
  /** (size, latency-ms) points, ascending by size. */
  points: ComplexityPoint[];
  /** Fitted exponent k in latency ≈ c·size^k (null when < 2 distinct points). */
  exponent: number | null;
  /** `true`/`false` vs the max-exponent bound, or null when no bound was set. */
  passed: boolean | null;
  /** The sweep process exit code (99 = exponent exceeded the bound). */
  exitCode: number;
}

/**
 * Fit the complexity exponent of `latency ≈ c · size^k` via least-squares on
 * log(size) vs log(latency) — a direct port of `fit_exponent` in
 * crates/loadr-cli/src/commands/sweep.rs. Needs ≥2 points with ≥2 distinct
 * positive sizes; returns null otherwise.
 */
export function fitExponent(points: ComplexityPoint[]): number | null {
  const pts = points
    .filter((p) => p.size > 0 && p.latencyMs > 0)
    .map((p) => [Math.log(p.size), Math.log(p.latencyMs)] as const);
  if (pts.length < 2) return null;
  const n = pts.length;
  const sx = pts.reduce((s, p) => s + p[0], 0);
  const sy = pts.reduce((s, p) => s + p[1], 0);
  const sxx = pts.reduce((s, p) => s + p[0] * p[0], 0);
  const sxy = pts.reduce((s, p) => s + p[0] * p[1], 0);
  const denom = n * sxx - sx * sx;
  if (Math.abs(denom) < Number.EPSILON) return null; // all sizes equal
  return (n * sxy - sx * sy) / denom;
}

/** Human verdict for a fitted exponent (mirrors `classify_exponent`). */
export function classifyExponent(k: number): string {
  if (k < 0.5) return 'flat / sub-linear';
  if (k < 1.2) return '≈ linear';
  if (k < 1.6) return 'super-linear';
  if (k < 2.4) return '≈ quadratic ⚠ DoS risk';
  return 'super-quadratic ⚠⚠ DoS';
}
