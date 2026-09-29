import { create } from 'zustand';
import type { KbHit, KbSource, KbStatus } from '../lib/types';

interface KnowledgeState {
  sources: KbSource[];
  status: KbStatus | null;
  query: string;
  hits: KbHit[];
  searching: boolean;
  /** Source shown in the detail pane, and the chunk to scroll to. */
  openSourceId: number | null;
  focusChunkId: number | null;

  setSources: (s: KbSource[]) => void;
  upsertSource: (s: KbSource) => void;
  removeSource: (id: number) => void;
  setStatus: (s: KbStatus) => void;
  setQuery: (q: string) => void;
  setHits: (h: KbHit[], searching?: boolean) => void;
  setSearching: (b: boolean) => void;
  openSource: (id: number | null, chunkId?: number) => void;
}

export const useKnowledge = create<KnowledgeState>((set) => ({
  sources: [],
  status: null,
  query: '',
  hits: [],
  searching: false,
  openSourceId: null,
  focusChunkId: null,

  setSources: (sources) => set({ sources }),
  upsertSource: (src) =>
    set((s) => {
      if (src.status === 'excluded') return { sources: s.sources.filter((x) => x.id !== src.id) };
      const exists = s.sources.some((x) => x.id === src.id);
      return {
        sources: exists ? s.sources.map((x) => (x.id === src.id ? src : x)) : [src, ...s.sources],
      };
    }),
  removeSource: (id) =>
    set((s) => ({
      sources: s.sources.filter((x) => x.id !== id),
      hits: s.hits.filter((h) => h.source_id !== id),
      openSourceId: s.openSourceId === id ? null : s.openSourceId,
    })),
  setStatus: (status) => set({ status }),
  setQuery: (query) => set({ query }),
  setHits: (hits, searching = false) => set({ hits, searching }),
  setSearching: (searching) => set({ searching }),
  openSource: (id, chunkId) => set({ openSourceId: id, focusChunkId: chunkId ?? null }),
}));
