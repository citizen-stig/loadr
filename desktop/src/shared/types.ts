// Structural types for a loadr test plan, mirroring the loadr-config schema
// (see `loadr schema`). These are compile-time ergonomics only — parsing keeps
// every key the YAML had, so unknown/future fields are never lost on round-trip.

export type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

export interface Plan {
  name?: string;
  description?: string;
  defaults?: Defaults;
  variables?: Record<string, Json>;
  secrets?: Record<string, Json>;
  data?: Record<string, Json>;
  metrics?: Record<string, Json>;
  js?: Json;
  scenarios?: Record<string, Scenario>;
  thresholds?: Record<string, string[]>;
  outputs?: Json[];
  plugins?: Array<PluginRef | string>;
}

export interface Defaults {
  http?: Record<string, Json>;
  tags?: Record<string, string>;
  think_time?: Json;
}

export type ExecutorKind =
  | 'constant-vus'
  | 'ramping-vus'
  | 'constant-arrival-rate'
  | 'ramping-arrival-rate'
  | 'per-vu-iterations'
  | 'shared-iterations'
  | 'externally-controlled';

export interface Scenario {
  executor: ExecutorKind;
  vus?: number;
  duration?: string;
  iterations?: number;
  stages?: Json[];
  rate?: number;
  start_vus?: number;
  start_rate?: number;
  time_unit?: string;
  pre_allocated_vus?: number;
  max_vus?: number;
  max_duration?: string;
  flow?: Step[];
  exec?: string;
}

// A flow step is a single-key mapping: { request: {...} } | { think_time: {...} } | …
export type StepKind =
  | 'request'
  | 'think_time'
  | 'js'
  | 'group'
  | 'repeat'
  | 'while'
  | 'if'
  | 'random'
  | 'foreach'
  | 'switch'
  | 'during'
  | 'retry'
  | 'parallel'
  | 'rendezvous';

export const STEP_KINDS: StepKind[] = [
  'request', 'think_time', 'js', 'group', 'repeat', 'while', 'if',
  'random', 'foreach', 'switch', 'during', 'retry', 'parallel', 'rendezvous',
];

export type Step = { [K in StepKind]?: Json } & Record<string, Json>;

export interface PluginRef {
  name: string;
}

/** The single step-kind key present on a flow step, or null if malformed. */
export function stepKind(step: Step): StepKind | null {
  const keys = Object.keys(step);
  const k = keys.find((key) => (STEP_KINDS as string[]).includes(key));
  return (k as StepKind) ?? null;
}
