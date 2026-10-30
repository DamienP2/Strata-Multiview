import { create } from 'zustand';

/** The Table whose editor is open (a modal over the Designer, the Data canvas included). */
export const useTableEditor = create<{ id: string | null; open: (id: string) => void; close: () => void }>((set) => ({
  id: null,
  open: (id) => set({ id }),
  close: () => set({ id: null }),
}));
