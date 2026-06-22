// Pure helpers for AI-assisted plan authoring: the system prompt, message
// builders, and extraction of a YAML plan from a model response. No network,
// no electron — the actual model call lives in main/ai.ts. Headless-testable.

import type { RepoContext } from './repo';

export const SYSTEM_PROMPT = `You are an expert performance engineer who writes load-test plans for "loadr".
Output a SINGLE loadr test plan as YAML, inside one \`\`\`yaml fenced block, and NOTHING else — no prose, no explanation.

loadr plan shape:
- top level: name, optional description, optional defaults.http.base_url, optional variables, optional data (CSV/JSON for data-driven), optional thresholds, and scenarios (a map).
- each scenario has an executor and a flow (ordered list of steps):
  - executors: constant-vus {vus,duration}, ramping-vus {stages:[{duration,target}]}, constant-arrival-rate {rate,duration,pre_allocated_vus}, per-vu-iterations {vus,iterations}, shared-iterations {vus,iterations}.
  - flow steps are single-key maps: request, think_time, js, group, repeat, while, if, foreach, switch, during, retry, parallel, rendezvous.
  - request: { method, url, name?, headers?, params?, body?, timeout?, assert?:[conditions], checks?:[conditions], extract?:[extractors] }.
    conditions: {type: status, equals|matches}, {type: jsonpath, expression, equals?|exists?}, {type: body_contains, value}, {type: duration, max}.
    extractors: {type: jsonpath|regex|header, name, expression|header}.
  - think_time: { type: constant|uniform|gaussian, duration | min,max | mean,std_dev }.
- thresholds gate pass/fail, e.g. { http_req_duration: ["p(95)<500"], http_req_failed: ["rate<0.01"] }.
- templates: \${var} interpolation; \${js: expr} for inline JS; session.vars for extracted values.

Rules:
- Prefer a closed model (constant-vus) with a realistic duration (e.g. 30s) and modest VUs unless the user asks otherwise.
- Use relative URLs with defaults.http.base_url when a base URL is known; otherwise full URLs.
- Add sensible assertions (status 2xx) and thresholds (p95 latency, error rate) by default.
- Only use the step kinds and fields listed above. Produce a plan that passes \`loadr validate\`.`;

export interface GenerateInput {
  prompt: string;
  schema?: unknown;
  repo?: RepoContext | null;
}

/** The user message for a fresh generation. */
export function buildUserMessage({ prompt, schema, repo }: GenerateInput): string {
  const parts: string[] = [];
  parts.push(prompt.trim() || 'Create a sensible HTTP load test.');
  if (repo?.summary) {
    parts.push(`Base the test on this repository. Cover its main endpoints with realistic load:\n\n${repo.summary}`);
  }
  if (schema) {
    parts.push(`Authoritative loadr JSON Schema (the plan MUST validate against it):\n${JSON.stringify(schema)}`);
  }
  parts.push('Return ONE ```yaml fenced block containing only the plan.');
  return parts.join('\n\n');
}

/** The follow-up message asking the model to fix validation errors. */
export function buildRepairMessage(yaml: string, diagnostics: { message: string }[]): string {
  const errs = diagnostics.map((d) => `- ${d.message}`).join('\n');
  return [
    'That plan failed `loadr validate` with these errors:',
    errs,
    'Here is the plan you produced:',
    '```yaml',
    yaml,
    '```',
    'Return a corrected plan as ONE ```yaml fenced block, nothing else.',
  ].join('\n\n');
}

/** Pull a YAML plan out of a model response (fenced block, else a bare plan). */
export function extractYaml(text: string): string | null {
  const fence = text.match(/```(?:ya?ml)?\s*\n?([\s\S]*?)```/i);
  if (fence && fence[1].trim()) return fence[1].trim();
  const t = text.trim();
  if (/^(name|scenarios|description|defaults)\s*:/m.test(t) && /\bscenarios\s*:/.test(t)) return t;
  return null;
}
