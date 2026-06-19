// The editing document: a single source of truth that keeps the structured
// plan model and the YAML text in sync, both ways.
//   • form edits  → apply(fn)/update(path,value) → re-serialize → YAML
//   • YAML edits  → setYaml(text) → parse → model (or surface a parse error)
// Validation is delegated (debounced) to the loadr CLI when running in Electron;
// in tests (no window.loadr) it simply stays null.

import { useCallback, useEffect, useRef, useState } from 'react';

import { parsePlan, serializePlan } from '../../shared/plan';
import { setIn, type Path } from '../../shared/edit';
import type { Plan } from '../../shared/types';
import type { ValidateResult } from '../../preload';

function safeParse(text: string): { plan: Plan; error: string | null } {
  try {
    return { plan: parsePlan(text), error: null };
  } catch (e) {
    return { plan: {}, error: (e as Error).message };
  }
}

export interface PlanDoc {
  yaml: string;
  plan: Plan;
  parseError: string | null;
  validation: ValidateResult | null;
  dirty: boolean;
  /** Replace the model via a pure function; YAML re-serializes to match. */
  apply: (fn: (p: Plan) => Plan) => void;
  /** Set a single value by path (form-field convenience). */
  update: (path: Path, value: unknown) => void;
  /** The text pane edited the YAML; re-parse into the model. */
  setYaml: (text: string) => void;
  /** Load a fresh document (open file) — resets dirty. */
  load: (text: string) => void;
  markSaved: () => void;
}

export function usePlanDoc(initialYaml = ''): PlanDoc {
  const init = safeParse(initialYaml);
  const [yaml, setYamlState] = useState(initialYaml);
  const [plan, setPlan] = useState<Plan>(init.plan);
  const [parseError, setParseError] = useState<string | null>(init.error);
  const [validation, setValidation] = useState<ValidateResult | null>(null);
  const [dirty, setDirty] = useState(false);

  const apply = useCallback((fn: (p: Plan) => Plan) => {
    setPlan((prev) => {
      const next = fn(prev);
      setYamlState(serializePlan(next));
      setParseError(null);
      setDirty(true);
      return next;
    });
  }, []);

  const update = useCallback((path: Path, value: unknown) => apply((p) => setIn(p, path, value)), [apply]);

  const setYaml = useCallback((text: string) => {
    setYamlState(text);
    setDirty(true);
    const { plan: p, error } = safeParse(text);
    setParseError(error);
    if (!error) setPlan(p);
  }, []);

  const load = useCallback((text: string) => {
    setYamlState(text);
    const { plan: p, error } = safeParse(text);
    setPlan(p);
    setParseError(error);
    setValidation(null);
    setDirty(false);
  }, []);

  const markSaved = useCallback(() => setDirty(false), []);

  // Debounced CLI validation (no-op outside Electron / when YAML is unparseable).
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    const api = (window as Window & { loadr?: { validate(y: string): Promise<ValidateResult> } }).loadr;
    if (!api?.validate || parseError) return;
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      api.validate(yaml).then(setValidation).catch(() => {});
    }, 400);
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [yaml, parseError]);

  return { yaml, plan, parseError, validation, dirty, apply, update, setYaml, load, markSaved };
}
