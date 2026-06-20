import { describe, expect, it } from 'vitest';
import type { DragEndEvent } from '@dnd-kit/core';

import { dragEndIndices } from './dnd';

const ev = (active: string, over: string | null): DragEndEvent =>
  ({ active: { id: active }, over: over === null ? null : { id: over } }) as unknown as DragEndEvent;

describe('dragEndIndices', () => {
  it('maps a drop to from/to indices', () => {
    expect(dragEndIndices(ev('2', '0'))).toEqual({ from: 2, to: 0 });
  });
  it('ignores a no-op drop (same slot)', () => {
    expect(dragEndIndices(ev('1', '1'))).toBeNull();
  });
  it('ignores a drop outside any target', () => {
    expect(dragEndIndices(ev('1', null))).toBeNull();
  });
  it('reorders within a scope', () => {
    expect(dragEndIndices(ev('then::2', 'then::0'))).toEqual({ from: 2, to: 0 });
  });
  it('ignores a cross-scope drop (different nested flows)', () => {
    expect(dragEndIndices(ev('then::1', 'else::0'))).toBeNull();
  });
});
