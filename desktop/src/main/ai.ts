// AI plan generation: orchestrate model call → extract YAML → `loadr validate`
// → one repair round, and the concrete Anthropic Messages API call. The
// orchestration takes its model and validator as injected functions (a seam),
// so it's unit-testable without network or electron. No electron imports here.

import { buildRepairMessage, buildUserMessage, extractYaml, SYSTEM_PROMPT, type GenerateInput } from '../shared/ai';
import { getProvider, type ProviderId } from '../shared/providers';

export interface ChatMessage { role: 'user' | 'assistant'; content: string }
export type ChatFn = (messages: ChatMessage[]) => Promise<string>;

export interface Diagnostic { severity: string; message: string }
export type ValidateFn = (yaml: string) => Promise<{ ok: boolean; diagnostics: Diagnostic[] }>;

export interface PlanResult {
  yaml: string;
  valid: boolean;
  repaired: boolean;
  diagnostics: Diagnostic[];
}

/**
 * Generate a plan: one model call, then if `loadr validate` rejects it, ONE
 * repair round feeding the errors back. Returns the best plan with its
 * validation state (the caller surfaces remaining diagnostics).
 */
export async function generatePlan(input: GenerateInput, chat: ChatFn, validate: ValidateFn): Promise<PlanResult> {
  const messages: ChatMessage[] = [{ role: 'user', content: buildUserMessage(input) }];
  const first = await chat(messages);
  let yaml = extractYaml(first);
  if (!yaml) throw new Error('The model did not return a YAML plan — try rephrasing the request.');

  let v = await validate(yaml);
  if (v.ok) return { yaml, valid: true, repaired: false, diagnostics: [] };

  const errors = v.diagnostics.filter((d) => d.severity === 'error');
  messages.push({ role: 'assistant', content: first });
  messages.push({ role: 'user', content: buildRepairMessage(yaml, errors) });
  const second = await chat(messages);
  const fixed = extractYaml(second);
  if (fixed) {
    yaml = fixed;
    v = await validate(yaml);
  }
  return { yaml, valid: v.ok, repaired: true, diagnostics: v.diagnostics };
}

// ---- provider transports ---------------------------------------------------
// Pure request-body mappers (unit-tested); the fetch wrappers below use them.

export function openAiBody(model: string, system: string, messages: ChatMessage[]) {
  return { model, messages: [{ role: 'system', content: system }, ...messages] };
}

export function googleBody(system: string, messages: ChatMessage[]) {
  return {
    systemInstruction: { parts: [{ text: system }] },
    contents: messages.map((m) => ({
      role: m.role === 'assistant' ? 'model' : 'user',
      parts: [{ text: m.content }],
    })),
  };
}

async function errorDetail(res: Response): Promise<string> {
  try {
    return (await res.json())?.error?.message ?? '';
  } catch {
    return '';
  }
}

async function anthropicChat(apiKey: string, model: string, messages: ChatMessage[]): Promise<string> {
  const res = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-api-key': apiKey, 'anthropic-version': '2023-06-01' },
    body: JSON.stringify({ model, max_tokens: 4096, system: SYSTEM_PROMPT, messages }),
  });
  if (!res.ok) throw new Error(`Anthropic API ${res.status}: ${await errorDetail(res)}`);
  const data = (await res.json()) as { content?: { type: string; text?: string }[] };
  return (data.content ?? []).filter((b) => b.type === 'text').map((b) => b.text ?? '').join('');
}

async function openAiChat(baseUrl: string, apiKey: string, model: string, messages: ChatMessage[]): Promise<string> {
  const res = await fetch(`${baseUrl}/chat/completions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', authorization: `Bearer ${apiKey}` },
    body: JSON.stringify(openAiBody(model, SYSTEM_PROMPT, messages)),
  });
  if (!res.ok) throw new Error(`API ${res.status}: ${await errorDetail(res)}`);
  const data = (await res.json()) as { choices?: { message?: { content?: string } }[] };
  return data.choices?.[0]?.message?.content ?? '';
}

async function googleChat(apiKey: string, model: string, messages: ChatMessage[]): Promise<string> {
  const url = `https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:generateContent?key=${encodeURIComponent(apiKey)}`;
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(googleBody(SYSTEM_PROMPT, messages)),
  });
  if (!res.ok) throw new Error(`Gemini API ${res.status}: ${await errorDetail(res)}`);
  const data = (await res.json()) as { candidates?: { content?: { parts?: { text?: string }[] } }[] };
  return (data.candidates?.[0]?.content?.parts ?? []).map((p) => p.text ?? '').join('');
}

/** Dispatch one chat turn to the selected provider's API. */
export async function providerChat(
  providerId: ProviderId,
  apiKey: string,
  model: string,
  messages: ChatMessage[],
): Promise<string> {
  const p = getProvider(providerId);
  switch (p.transport) {
    case 'anthropic':
      return anthropicChat(apiKey, model, messages);
    case 'openai':
      return openAiChat(p.baseUrl ?? 'https://api.openai.com/v1', apiKey, model, messages);
    case 'google':
      return googleChat(apiKey, model, messages);
    default:
      throw new Error(`unknown provider transport: ${p.transport}`);
  }
}
