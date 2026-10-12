import type { BetInput } from '@commit-casino/casino-contracts';
import { createStore } from 'zustand/vanilla';

export interface DraftState { bets: BetInput[]; undoStack: BetInput[][]; chip: number; replace: (bets: readonly BetInput[]) => void; place: (betId: string) => void; undo: () => void; clear: () => void; setChip: (chip: number) => void; fresh: () => void }
export const createDraftStore = () => createStore<DraftState>((set, get) => ({
  bets: [], undoStack: [], chip: 100,
  replace: bets => set(state => ({ bets: bets.map(b => ({ ...b })), undoStack: [...state.undoStack, state.bets] })),
  place: betId => { const state = get(); const previous = state.bets.find(b => b.betId === betId); state.replace(previous ? state.bets.map(b => b.betId === betId ? { ...b, stake: b.stake + state.chip } : b) : [...state.bets, { betId, stake: state.chip }]); },
  undo: () => set(state => ({ bets: state.undoStack.at(-1) ?? state.bets, undoStack: state.undoStack.slice(0, -1) })),
  clear: () => get().replace([]),
  setChip: chip => set({ chip }),
  fresh: () => set({ bets: [], undoStack: [] }),
}));
