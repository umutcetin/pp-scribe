import { create } from 'zustand';
import type { Caption, MeetingMeta, RefineProgress, Settings, Speaker } from '../lib/types';

export type RecState = 'idle' | 'starting' | 'recording' | 'finalizing';

export interface Toast {
  id: number;
  kind: 'info' | 'warn' | 'error';
  text: string;
}

interface AppState {
  settings: Settings | null;
  meetings: MeetingMeta[];
  /** Meeting shown in the main pane when not looking at the live view. */
  selectedId: string | null;
  view: 'live' | 'meeting' | 'knowledge';
  /** Scroll target when a meeting is opened from a search hit. */
  meetingFocusMs: number | null;

  recState: RecState;
  active: MeetingMeta | null;
  /** Local timestamp corresponding to meeting t=0, for the running timer. */
  activeT0: number;
  captions: Record<string, Caption>;
  /** Live caption ids already put on the clipboard, for "copy new". */
  copiedIds: Record<string, true>;
  levels: Record<Speaker, number>;

  refine: RefineProgress | null;
  toasts: Toast[];
  settingsOpen: boolean;

  setSettings: (s: Settings) => void;
  setMeetings: (m: MeetingMeta[]) => void;
  upsertMeeting: (m: MeetingMeta) => void;
  removeMeeting: (id: string) => void;
  select: (id: string | null, focusMs?: number) => void;
  showLive: () => void;
  showKnowledge: () => void;

  setRecState: (s: RecState) => void;
  beginMeeting: (meta: MeetingMeta, elapsedMs: number, captions: Caption[]) => void;
  endMeeting: () => void;
  applyCaption: (c: Caption) => void;
  markCopied: (ids: string[]) => void;
  setLevel: (speaker: Speaker, level: number) => void;

  setRefine: (p: RefineProgress | null) => void;
  toast: (kind: Toast['kind'], text: string) => void;
  dismissToast: (id: number) => void;
  setSettingsOpen: (open: boolean) => void;
}

let toastId = 0;

export const useApp = create<AppState>((set) => ({
  settings: null,
  meetings: [],
  selectedId: null,
  view: 'live',
  meetingFocusMs: null,

  recState: 'idle',
  active: null,
  activeT0: 0,
  captions: {},
  copiedIds: {},
  levels: { me: 0, them: 0 },

  refine: null,
  toasts: [],
  settingsOpen: false,

  setSettings: (settings) => set({ settings }),
  setMeetings: (meetings) => set({ meetings }),
  upsertMeeting: (m) =>
    set((s) => {
      const exists = s.meetings.some((x) => x.id === m.id);
      const meetings = exists
        ? s.meetings.map((x) => (x.id === m.id ? m : x))
        : [m, ...s.meetings];
      return { meetings, active: s.active?.id === m.id ? m : s.active };
    }),
  removeMeeting: (id) =>
    set((s) => ({
      meetings: s.meetings.filter((m) => m.id !== id),
      selectedId: s.selectedId === id ? null : s.selectedId,
      view: s.selectedId === id ? 'live' : s.view,
    })),
  select: (id, focusMs) => set({ selectedId: id, view: id ? 'meeting' : 'live', meetingFocusMs: focusMs ?? null }),
  showLive: () => set({ view: 'live' }),
  showKnowledge: () => set({ view: 'knowledge' }),

  setRecState: (recState) => set({ recState }),
  beginMeeting: (meta, elapsedMs, captions) =>
    set({
      recState: 'recording',
      active: meta,
      activeT0: Date.now() - elapsedMs,
      captions: Object.fromEntries(captions.map((c) => [c.id, c])),
      copiedIds: {},
      levels: { me: 0, them: 0 },
      view: 'live',
    }),
  endMeeting: () => set({ recState: 'idle', active: null, levels: { me: 0, them: 0 } }),
  applyCaption: (c) =>
    set((s) => {
      if (s.active && c.meeting_id && s.active.id !== c.meeting_id) return {};
      const captions = { ...s.captions };
      if (c.is_final && !c.text) {
        delete captions[c.id];
      } else if (!c.is_final && captions[c.id]?.is_final) {
        return {}; // never let a late preview overwrite final text
      } else {
        captions[c.id] = c;
      }
      return { captions };
    }),
  markCopied: (ids) =>
    set((s) => ({ copiedIds: { ...s.copiedIds, ...Object.fromEntries(ids.map((id) => [id, true as const])) } })),
  setLevel: (speaker, level) => set((s) => ({ levels: { ...s.levels, [speaker]: level } })),

  setRefine: (refine) => set({ refine }),
  toast: (kind, text) => {
    const id = ++toastId;
    set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
    setTimeout(() => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })), kind === 'error' ? 9000 : 5000);
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
}));
