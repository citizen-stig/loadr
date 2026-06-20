import { createContext, useContext } from 'react';

// Shared selection between the outline and the form cards: clicking an outline
// node selects an anchor id; the matching form card highlights itself.
export interface Selection {
  selectedId: string | null;
  select: (id: string) => void;
}

export const SelectionContext = createContext<Selection>({ selectedId: null, select: () => {} });
export const useSelection = (): Selection => useContext(SelectionContext);
