import type { DragEndEvent } from '@dnd-kit/core';

// Sortable items carry an id of the form `<scope>::<index>` (a bare `<index>`
// is treated as the empty scope). The scope lets several nested flow editors
// share one page without their indices colliding: a drop only reorders when
// both ends live in the same scope. Pure so it's headless-testable; the
// moveStepAt it feeds is covered in edit.test.ts and the dnd-kit wiring is
// verified e2e.
function parseId(id: string | number): { scope: string; idx: number } {
  const s = String(id);
  const at = s.lastIndexOf('::');
  return at < 0 ? { scope: '', idx: Number(s) } : { scope: s.slice(0, at), idx: Number(s.slice(at + 2)) };
}

export function dragEndIndices(event: DragEndEvent): { from: number; to: number } | null {
  const a = parseId(event.active.id);
  const b = event.over ? parseId(event.over.id) : null;
  if (!b || a.scope !== b.scope) return null;
  if (Number.isNaN(a.idx) || Number.isNaN(b.idx) || a.idx === b.idx) return null;
  return { from: a.idx, to: b.idx };
}
